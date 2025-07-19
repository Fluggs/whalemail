use std::io;
use std::io::{Error, ErrorKind};
use std::net::SocketAddr;
use std::str;
use tokio::net::TcpStream;
use crate::smtp::Smtp;

pub struct ConnectionHandler<'a>{
    socket: &'a TcpStream,
    addr: &'a SocketAddr,
    expect: Option<String>,
}

impl ConnectionHandler<'_> {
    pub fn new<'a> (socket: &'a TcpStream, addr: &'a SocketAddr) -> ConnectionHandler<'a> {
        ConnectionHandler {
            socket,
            addr,
            expect: None,
        }
    }
    
    pub async fn process_socket(&self) -> io::Result<()> {
        let mut smtp = Smtp::new(self);
        smtp.init_smtp().await?;
        
        loop {
            if smtp.closed {
                // drop closes socket
                break;
            }
            self.socket.readable().await?;

            let mut buf = [0; 4096];
            match self.socket.try_read(&mut buf) {
                Ok(0) => {
                    println!("Connection closed by client.");
                    break
                },
                Ok(n) => {
                    let v = match str::from_utf8(&buf) {
                        Ok(v) => v.to_string(),
                        Err(_) => {
                            let s = buf[..n].iter()
                                .map(|x| format!("{:02x?}", x))
                                .collect::<Vec<_>>()
                                .join(" ");
                            format!("Invalid unicode: {s}")
                        }
                    };

                    smtp.handle(v).await;
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
                        println!("{err}");
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
        println!("Trying to send {msg}");
        
        Ok(())
    }
}