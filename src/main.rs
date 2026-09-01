mod smtp {
    pub(crate) mod server;
    pub(crate) mod client;
    pub(crate) mod common;
    pub(crate) mod error;
    pub(crate) mod envelope;
}

mod auth {
    pub(crate) mod auth;
}

mod userdb {
    pub(crate) mod drivers {
        pub(crate) mod postgres;
        #[cfg(test)]
        pub(crate) mod mock_db;
    }
    pub(crate) mod userdb;
}

mod net;
mod maildir;
mod config;
mod tls;
mod user;

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
use tokio_rustls::TlsAcceptor;
use crate::userdb::userdb::UserDBMtx;
use crate::config::Config;
use crate::net::IO;
use crate::userdb::drivers::postgres::Postgres;

struct TlsListener {
    acceptor: TlsAcceptor,
    tcp_listener: TcpListener 
}

#[tokio::main(flavor = "multi_thread")]
async fn main() -> io::Result<()> {
    let config = match Config::load() {
        Ok(config) => config,
        Err(err) => {
            panic!("Error loading config: {}", err)
        }
    };
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(config.log_level.clone())).init();
    
    debug!(target: "blub", "yam!");
    let user_db = match Postgres::new(config.userdb_config.clone()).await {
        Ok(r) => r,
        Err(err) => panic!("Error building user db: {:?}", err)
    };

    let listener = TcpListener::bind(config.bind_ip.clone()).await.or_else(|err| {
        println!("Binding to {} failed.", &config.bind_ip);
        Err(err)
    })?;

    // Build TlsListener if config values for certs are provided
    let tls_listener = match (&config.cert_dir, &config.trusted_ca_cert_dir) {
        (Some(_cert_dir), Some(_ca_dir)) => {
            let acceptor = tls::build_tls_acceptor(
                config.cert_dir.clone().unwrap(),
                config.trusted_ca_cert_dir.clone().unwrap()
            );
            
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
    Accepts via tls_listener in case it is Some() and returns its resulting (stream, sock).
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
                        eprintln!("Error on incoming connection on TLS port: '{err}'");
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
                    Ok((socket, addr)) => process_socket_silent(
                        socket,
                        addr,
                        config.clone(),
                        user_db.clone(),
                    ).await,
                    Err(err) => eprintln!("Error processing plain socket: '{err}'")
                }
            },
            
            Some((tcp_stream, addr)) = conditional_tls_accept(tls_listener.as_ref()) => {
                let tls_listener = tls_listener.as_ref().expect("TLS not configured");
                debug!("New connection on tls port from {}", addr);
                let tls_acceptor = &tls_listener.acceptor.clone();
                match tls_acceptor.accept(tcp_stream).await {
                    Ok(stream) => process_socket_silent(
                        stream,
                        addr,
                        config.clone(),
                        user_db.clone(),
                    ).await,
                    Err(err) => eprintln!("Error accepting TLS stream: '{}'", err)
                }
            }
        }
    }
}

async fn process_socket_silent<T: IO>(stream: T, addr: SocketAddr, config: Config, user_db: UserDBMtx) {
    let handler = ConnectionHandler::new(stream, addr);
    debug!("Incoming client: {}:{}", handler.addr.ip(), handler.addr.port());
    match handler.server_loop(config, user_db).await {
        Ok(()) => (),
        Err(err) => eprintln!("Socket came back with error: '{err}'")
    }
    println!("Closing connection from {}:{}", addr.ip(), addr.port());
}
