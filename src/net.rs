use std::io;
use std::io::{Error, ErrorKind};
use std::net::SocketAddr;
use std::str;
use log::{debug, info};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use crate::userdb::userdb::UserDBMtx;
use crate::config::Config;
use crate::smtp::server::{SmtpServer, StateKind};
use crate::maildir::Storage;
use crate::smtp::client::SmtpClient;

pub trait IO: AsyncRead + AsyncWrite + Unpin {}
impl<T: AsyncReadExt + AsyncWriteExt + Unpin> IO for T {}

enum Either<T: IO> {
    Server(SmtpServer<T>),
    Client(SmtpClient<T>)
}

impl<T: IO> Either<T> {
    async fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            Either::Server(server) => server.connhandler_mut().read(buf).await,
            Either::Client(client) => client.connhandler_mut().read(buf).await
        }
    }
    
    async fn handle(self, input: String) -> Result<(Either<T>, StateKind), Error> {
        match self {
            Either::Server(server) => server.handle(input).await
                .and_then(|(server, state_kind)| Ok((Either::Server(server), state_kind))),
            Either::Client(client) => client.handle(input).await
                .and_then(|(client, state_kind)| Ok((Either::Client(client), state_kind))),
        }
    }
}

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
    
    pub(crate) async fn server_loop(self, config: Config, user_db: UserDBMtx) -> io::Result<()> {
        let maildir_config = (&config.maildir_config).clone();
        let hostname = config.hostname.clone();
        let smtp = Either::Server(SmtpServer::new(
            self,
            config,
            user_db,
            Storage::new(hostname, maildir_config)
        )
            .await?);
        
        Self::socket_loop(smtp).await
    }
    
    pub(crate) async fn client_loop(self, client: SmtpClient<T>) -> io::Result<()> {
        Self::socket_loop(Either::Client(client)).await
    }
    
    async fn socket_loop(mut smtp: Either<T>) -> io::Result<()> {
        loop {

            let mut buf = [0; 4096];
            match smtp.read(&mut buf).await? {
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
