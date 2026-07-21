use std::io;
use std::io::{Error, ErrorKind};
use std::net::SocketAddr;
use std::str;
use tokio::net::TcpStream;
use log::{debug, info};
use crate::auth::userdb::{UserDBMtx};
use crate::smtp::smtp::{Smtp, StateKind};
use crate::smtp::smtp_error::SmtpError;
use crate::storage::Storage;

pub struct ConnectionHandler {
    socket: TcpStream,
    pub(crate) addr: SocketAddr,
}

impl ConnectionHandler {
    pub fn new<'a> (socket: TcpStream, addr: SocketAddr) -> ConnectionHandler {
        ConnectionHandler {
            socket,
            addr,
        }
    }
    
    pub async fn process_socket(self, user_db: UserDBMtx) -> io::Result<()> {
        let mut smtp = Smtp::new(self, user_db, Storage { directory: "todo_dir".to_string() });
        smtp.init_smtp().await
            .or_else(|error: SmtpError| Err(error.io_error.unwrap()))?;
        
        loop {
            if smtp.closed {
                // drop closes socket
                break;
            }
            smtp.conn.as_ref().unwrap().socket.readable().await?;

            let mut buf = [0; 4096];
            match smtp.conn.as_ref().unwrap().socket.try_read(&mut buf) {
                Ok(0) => {
                    info!("Connection closed by client.");
                    break
                },
                Ok(n) => {
                    debug!("---- Reading {n} bytes");
                    let v = match str::from_utf8(&buf[..n]) { // todo consider from_utf8_lossy
                        Ok(v) => v.to_string(),
                        Err(_) => {
                            let s = buf[..n].iter()
                                .map(|x| format!("{:02x?}", x))
                                .collect::<Vec<_>>()
                                .join(" ");
                            format!("Invalid unicode: {s}")
                        }
                    };

                    match smtp.handle(v).await {
                        Ok(StateKind::QUIT) => break,
                        Ok(StateKind::CONTINUE) => (),
                        Err(e) => return Err(e.into())
                    };
                }

                Err(ref e) if e.kind() == ErrorKind::WouldBlock => {
                    continue;
                }
                Err(e) => {
                    return Err(e.into());
                }
            }
        }

        Ok(())
    }
    
    pub async fn send(&self, msg: String) -> io::Result<()> {
        self.socket.writable().await?;

        loop {
            self.socket.writable().await?;
            let msg = msg.as_bytes();
            
            match self.socket.try_write(msg) {
                Ok(n) => {
                    if n < msg.len() {
                        let err = format!("Tried to write {} bytes but only {} were written.", msg.len(), n);
                        debug!("{err}");
                        Error::new(ErrorKind::Other, err);
                    }
                    break;
                }
                Err(ref e) if e.kind() == ErrorKind::WouldBlock => {
                    continue;
                }
                Err(e) => {
                    return Err(e.into());
                }
            }
        }
        debug!("---- {msg}");
        
        Ok(())
    }
}