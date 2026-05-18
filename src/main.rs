mod smtp;
mod net;
mod util;
mod tests;
mod smtp_message;
mod smtp_error;

use tokio::net::{TcpListener, TcpStream};
use std::io;
use std::net::SocketAddr;
use net::ConnectionHandler;
use env_logger;
use log;
use log::debug;

#[tokio::main(flavor = "current_thread")]
async fn main() -> io::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("debug")).init();
    debug!(target: "blub", "yam!");

    let listener = TcpListener::bind("127.0.0.1:3000").await?;
    
    loop {
        match listener.accept().await {
            Ok((socket, addr)) => process_socket_silent(socket, addr).await,
            Err(err) => println!("{err}")
        }
    }
}

async fn process_socket_silent(socket: TcpStream, addr: SocketAddr) {
    println!("Socket from {}:{}", addr.ip(), addr.port());
    let handler = ConnectionHandler::new(socket, addr);
    match handler.process_socket().await {
        Ok(()) => (),
        Err(err) => eprintln!("Socket came back with error: '{err}'")
    }
    println!("Closing connection from {}:{}", addr.ip(), addr.port());
}
