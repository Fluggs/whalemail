mod smtp;
mod net;
mod util;

use tokio::net::{TcpListener, TcpStream};
use std::io;
use std::str;
use std::net::SocketAddr;
use net::ConnectionHandler;

#[tokio::main(flavor = "current_thread")]
async fn main() -> io::Result<()> {
    let s = "helo\n";
    let p = "ho\r\n";
    println!("s: '{}', p: '{}'", s.trim(), p.trim());
    let q = p.trim();
    let b: [u8; 12] = [0x43, 0x68, 0x72, 0x69, 0x73, 0x73, 0x69, 0x0d, 0x0a, 0x00, 0x00, 0x00];
    let bs = str::from_utf8(&b[..9]).unwrap();
    let bst = bs.trim();
    println!("bs: {} / {}", util::string_as_bytes(&bs.to_string()), util::string_as_bytes(&bst.to_string()));
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
    println!("Closing connection from {}:{}", addr.ip(), addr.port());
}
