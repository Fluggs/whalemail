use tokio::net::{TcpListener, TcpStream};
use std::io;
use std::str;
use std::net::SocketAddr;

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
    match process_socket(socket, addr).await {
        Ok(()) => (),
        Err(err) => println!("{err}")
    }
}

async fn process_socket(socket: TcpStream, addr: SocketAddr) -> io::Result<()> {
    loop {
        socket.readable().await?;
        
        let mut buf = [0; 4096];
        match socket.try_read(&mut buf) {
            Ok(0) => {
                println!("len 0");
                break
            },
            Ok(n) => {
                println!("read {} bytes", n);
                for el in buf {
                    println!("{el}");
                    if el == 0 {
                        println!("(end)");
                        break;
                    }
                }
                let v = match str::from_utf8(&buf) {
                    Ok(v) => v.to_string(),
                    Err(err) => {
                        format!("{err}")
                    }
                };
                println!("read: {}", v);
            }
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                continue;
            }
            Err(e) => {
                return Err(e.into());
            }
        }
    }
    
    println!("done processing");
    
    Ok(())
}
