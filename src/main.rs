mod smtp;
mod net;

use tokio::net::{TcpListener, TcpStream};
use std::io;
use std::net::SocketAddr;
use net::ConnectionHandler;

#[tokio::main(flavor = "current_thread")]
async fn main() -> io::Result<()> {
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
    let handler = ConnectionHandler::new(&socket, &addr);
    match handler.process_socket().await {
        Ok(()) => (),
        Err(err) => println!("{err}")
    }
}
