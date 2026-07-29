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
mod tls;

mod tests {
    pub(crate) mod test;
    mod tests_auth;
    mod tests_smtp;
}

use tokio::net::{TcpListener, TcpSocket, TcpStream};
use std::io;
use std::net::SocketAddr;
use net::ConnectionHandler;
use env_logger;
use log;
use log::{debug};
use rustls::SupportedCipherSuite::Tls12;
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio_rustls::TlsAcceptor;
use crate::auth::userdb::{UserDB, UserDBMtx};

struct TlsListener {
    acceptor: TlsAcceptor,
    tcp_listener: TcpListener 
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> io::Result<()> {
    let config = config::Config::load();
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(config.log_level.clone())).init();
    
    debug!(target: "blub", "yam!");
    let user_db = UserDB::new();

    let listener = TcpListener::bind(config.bind_ip.clone()).await.or_else(|err| {
        println!("Binding to {} failed.", &config.bind_ip);
        Err(err)
    })?;

    // Build TlsListener if config values for certs are provided
    let tls_listener = match (&config.cert_dir, &config.trusted_ca_cert_dir) {
        (Some(cert_dir), Some(ca_dir)) => {
            let acceptor = tls::build_tls_acceptor(config.cert_dir.unwrap(), config.trusted_ca_cert_dir.unwrap());
            let listener = TcpListener::bind(config.bind_ip_tls.clone())
                .await
                .or_else(|err| {
                    println!("Binding to {} failed.", &config.bind_ip_tls);
                    Err(err)
                })?;
            Some(TlsListener {
                acceptor,
                tcp_listener: listener
            })
        }
        _ => None
    };
    
    /**
    Accepts on tls_listener in case it is Some() and returns its resulting (stream, sock).
    Returns None otherwise.
    */
    async fn conditional_tls_accept(tls_listener: Option<&TlsListener>) -> Option<(TcpStream, SocketAddr)> {
        match tls_listener {
            // TLS is configured
            Some(tls_listener) => {
                match tls_listener.tcp_listener.accept().await {
                    Ok((stream, addr)) => {
                        Some((stream, addr))
                    },
                    Err(err) => {
                        eprintln!("Error on incoming connection on TLS port: {err}");
                        None
                    }
                }
            },

            // No TLS configured
            None => None
        }
    }

    loop {
        tokio::select! {
            plain = listener.accept() => {
                match plain {
                    Ok((socket, addr)) => process_socket_silent(socket, addr, user_db.clone(), config.maildir_root.clone()).await,
                    Err(err) => println!("{err}")
                }
            },
            
            Some((tcp_stream, addr)) = conditional_tls_accept(tls_listener.as_ref()) => {
                let tls_listener = tls_listener.as_ref().expect("TLS not configured");
                debug!("New connection on tls port from {}", addr);
                let tls_acceptor = &tls_listener.acceptor.clone();
                let tlsstream = tls_acceptor.accept(tcp_stream).await?;
            }
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
