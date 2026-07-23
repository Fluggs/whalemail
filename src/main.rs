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
mod config;

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
use log::{debug};
use crate::auth::userdb::{UserDB, UserDBMtx};

#[tokio::main(flavor = "current_thread")]
async fn main() -> io::Result<()> {
    let config = config::Config::load();

    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(config.log_level.clone())).init();
    debug!(target: "blub", "yam!");
    let user_db = UserDB::new();

    let listener = TcpListener::bind(config.bind_ip.clone()).await.or_else(|e| {
        println!("Binding to {} failed.", &config.bind_ip);
        Err(e)
    })?;
    
    loop {
        match listener.accept().await {
            Ok((socket, addr)) => process_socket_silent(socket, addr, user_db.clone(), config.maildir_root.clone()).await,
            Err(err) => println!("{err}")
        }
    }
}

async fn process_socket_silent(socket: TcpStream, addr: SocketAddr, user_db: UserDBMtx, storage_dir: String) {
    let handler = ConnectionHandler::new(socket, addr);
    debug!("Incoming client: {}:{}", handler.addr.ip(), handler.addr.port());
    match handler.process_socket(user_db, storage_dir).await {
        Ok(()) => (),
        Err(err) => eprintln!("Socket came back with error: '{err}'")
    }
    println!("Closing connection from {}:{}", addr.ip(), addr.port());
}
