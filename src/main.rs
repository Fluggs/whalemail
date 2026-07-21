mod smtp {
    pub(crate) mod smtp;
    pub(crate) mod smtp_error;
    pub(crate) mod smtp_mail;
}

mod auth {
    pub(crate) mod auth;
    pub(crate) mod userdb;
}

mod net;
mod storage;
mod tests {
    pub(crate) mod test;
    mod tests_auth;
    mod tests_smtp;
}

use tokio::net::{TcpListener, TcpStream};
use std::io;
use std::net::SocketAddr;
use net::ConnectionHandler;
use env_logger;
use log;
use log::debug;
use crate::auth::userdb::{UserDB, UserDBMtx};

#[tokio::main(flavor = "current_thread")]
async fn main() -> io::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("debug")).init();
    debug!(target: "blub", "yam!");
    
    let user_db = UserDB::new();

    let listener = TcpListener::bind("127.0.0.1:3000").await?;
    
    loop {
        match listener.accept().await {
            Ok((socket, addr)) => process_socket_silent(socket, addr, user_db.clone()).await,
            Err(err) => println!("{err}")
        }
    }
}

async fn process_socket_silent(socket: TcpStream, addr: SocketAddr, user_db: UserDBMtx) {
    let handler = ConnectionHandler::new(socket, addr);
    debug!("Incoming client: {}:{}", handler.addr.ip(), handler.addr.port());
    match handler.process_socket(user_db).await {
        Ok(()) => (),
        Err(err) => eprintln!("Socket came back with error: '{err}'")
    }
    println!("Closing connection from {}:{}", addr.ip(), addr.port());
}
