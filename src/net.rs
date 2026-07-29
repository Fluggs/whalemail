use std::io;
use std::io::{Error, ErrorKind};
use std::net::SocketAddr;
use std::str;
use log::{debug, info};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use crate::auth::userdb::{UserDBMtx};
use crate::smtp::smtp::{Smtp, StateKind};
use crate::smtp::smtp_error::SmtpError;
use crate::storage::Storage;

pub(crate) trait IO: AsyncRead + AsyncWrite + Unpin {}
impl<T: AsyncReadExt + AsyncWriteExt + Unpin> IO for T {}

pub struct ConnectionHandler<T: IO> {
    socket: T,
    pub(crate) addr: SocketAddr,
}

impl<T: IO> ConnectionHandler<T> {
    pub fn new<'a> (socket: T, addr: SocketAddr) -> ConnectionHandler<T> {
        ConnectionHandler {
            socket,
            addr,
        }
    }

    async fn read(&mut self, buf: & mut [u8]) -> io::Result<usize> {
        self.socket.read(buf).await
    }
    
    pub async fn process_socket(self, user_db: UserDBMtx, storage_dir: String,) -> io::Result<()> {
        let mut smtp = Smtp::new(self, user_db, Storage { directory: storage_dir });
        smtp.init_smtp()
            .await
            .or_else(|error: SmtpError| Err(error.io_error.unwrap()))?;
        
        loop {
            if smtp.closed {
                // drop closes socket
                break;
            }

            let mut buf = [0; 4096];
            match smtp.connhandler_mut().read(&mut buf).await? {
                0 => {
                    info!("Connection closed by client.");
                    break
                },
                n => {
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
                },
            }
        }

        Ok(())
    }
    
    pub async fn send(&mut self, msg: String) -> io::Result<()> {
        match self.socket.write(msg.as_bytes()).await? {
            n if n < msg.len() => {
                let err = format!("Tried to write {} bytes but only {} were written.", msg.len(), n);
                debug!("{err}");
                Err(Error::new(ErrorKind::Other, err))
            },
            _ => Ok(())
        }
    }
}