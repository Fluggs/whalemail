use std::io;
use std::io::{Error, ErrorKind};
use std::net::SocketAddr;
use std::str;
use log::{debug, info};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpStream;
use crate::userdb::userdb::UserDBMtx;
use crate::config::Config;
use crate::smtp::server::{SmtpServer, StateKind};
use crate::maildir::Storage;
use crate::queue::queue::QueueMtx;

pub trait IO: AsyncRead + AsyncWrite + Unpin {}
impl<T: AsyncReadExt + AsyncWriteExt + Unpin> IO for T {}

pub struct ConnectionHandler<T: IO> {
    pub(crate) socket: T,
    pub(crate) addr: SocketAddr,
}

impl<T: IO> ConnectionHandler<T> {
    pub(crate) fn new<'a> (socket: T, addr: SocketAddr) -> ConnectionHandler<T> {
        ConnectionHandler {
            socket,
            addr,
        }
    }
    
    pub(crate) async fn connect(host: &String) -> io::Result<ConnectionHandler<TcpStream>> {
        let sock = TcpStream::connect(host).await?;
        let addr = sock.peer_addr()?;
        Ok(ConnectionHandler::new(sock, addr))
    }

    pub(crate) async fn read_into(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.socket.read(buf).await
    }
    
    pub(crate) async fn read(&mut self) -> io::Result<String> {
        let mut buf = [0; 4096];
        match self.read_into(&mut buf).await? {
            0 => Ok(String::new()),
            n => Ok(String::from(String::from_utf8_lossy(&buf[..n])))
        }
    }
    
    pub(crate) async fn server_loop(self, config: Config, user_db: UserDBMtx, queue: QueueMtx) -> io::Result<()> {
        let maildir_config = (&config.maildir_config).clone();
        let hostname = config.hostname.clone();
        let smtp = SmtpServer::new(self, config, user_db, Storage::new(hostname, maildir_config), queue)
            .await?;
        
        Self::socket_loop(smtp).await
    }
    
    async fn socket_loop(mut smtp: SmtpServer<T>) -> io::Result<()> {
        loop {
            let mut buf = [0; 4096];
            match smtp.connhandler_mut().read_into(&mut buf).await? {
                0 => {
                    info!("Connection closed by client.");
                    break
                },
                n => {
                    debug!("---- Reading {n} bytes: {}", String::from_utf8_lossy(&buf[..n]));
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

                    smtp = match smtp.handle(v).await {
                        Ok((_, StateKind::QUIT)) => break,
                        Ok((smtp, StateKind::CONTINUE)) => smtp,
                        Err(e) => return Err(e.into())
                    };
                },
            }
        }

        Ok(())
    }
    
    pub async fn send(&mut self, msg: String) -> io::Result<()> {
        debug!("-- Sending: {:?}", msg);
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
