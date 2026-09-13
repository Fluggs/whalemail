mod smtp {
    pub(crate) mod server;
    pub(crate) mod client;
    pub(crate) mod error;
    pub(crate) mod envelope;
}

mod auth;
mod userdb;
mod queue;
mod net;
mod maildir;
mod config;
mod tls;
mod user;

mod tests {
    pub(crate) mod test;
    mod tests_auth;
    mod tests_server;
    mod tests_client;
}

use tokio::net::{TcpListener, TcpStream};
use std::io;
use std::net::SocketAddr;
use net::ConnectionHandler;
use log::{debug};
use tokio_rustls::TlsAcceptor;
use crate::userdb::UserDBMtx;
use crate::config::Config;
use crate::net::IO;
use crate::queue::{Queue, QueueMtx};
use crate::userdb::drivers::postgres::Postgres;

struct TlsListener {
    acceptor: TlsAcceptor,
    tcp_listener: TcpListener 
}

// Multi-threaded is on by default, so you never need to specify
#[tokio::main(flavor = "multi_thread")]
// You can specify its an std::io::Result but it's very unusual - especially under tokio which has it's own io errors.
// Instead I would use `anyhow::Result<()>`.
async fn main() -> io::Result<()> {
    let config = match Config::load() {
        Ok(config) => config,
        Err(err) => {
            // This is where I would expect panic handling to happen.
            // Make sure to have very clear locations in the code for translating and handling types (not just errors).
            // Also, with anyhow you can do `err.context("Error loading config").unwrap()`
            panic!("Error loading config: {}", err)
        }
    };
    // I would expect a lot of these stages to go into named functions. It makes reading the code easier.
    // setup_logging(&config);
    // Overall it looks ok I think, but it's very vanilla and is probably already handled as a
    // helper function in env_logger. I don't know for sure though.
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(config.log_level.clone())).init();
    
    debug!(target: "blub", "yam!");
    let user_db = match Postgres::build(config.userdb_config.clone()).await {
        Ok(r) => r,
        Err(err) => panic!("Error building user db: {:?}", err)
    };
    
    // Queues are an excellent way to learn the magic behind rust.
    // That's largely because when used correctly, they don't need to be wrapped in Mutex or RwLock etc.
    //
    // You have the following types of channels (queues)
    // mpsc = the most common "queue" or channel. It means multiple producer, single consumer.
    //      It means you can clone the sender side of the channel without locks and Arcs,
    //      and the receiver will be able to consume without locks
    // broadcast = a single sender can set a value, all receivers read the value without locks
    // oneshot = a sender can send a single message and then it closes the channel. The receiver blocks until it receives a message.
    //      This is used a lot in message callbacks, or execution control (start-stop commands across actors/threads).
    // 
    // The standard library has channels, and for the most part they are great.
    // Some people want even more performance out of channels and so there are variaitons
    // `tokio` provides channels that are async. That is incredibly useful for async code with lots of actors/tasks.
    // `crossbeam` and `flume` are also great alternatives worth looking into.
    // Personally I use the standard library ones or tokio ones if I am using tokio.
    // In production we have used all.
    let queue = Queue::new(config.clone());

    let listener = TcpListener::bind(config.bind_ip.clone()).await.inspect_err(|err| {
        // Above on line 58 you actually did a great job of using the logger. Rewriting this line to show how to use it for this case.
        // I am too tired to fiddle with this, but it would basically look like that.
        // We also tend to use the `tracing` crate for more convenient macros for the same effect.
        log::error!(bind_ip: config.bind_ip, err; "Binding failed");
        // You may also want to use `eprintln` if it's error related
        println!("Binding to {} failed: {}", &config.bind_ip, err);
    })?;

    // Build TlsListener if config values for certs are provided
    let tls_listener = match (&config.cert_dir, &config.trusted_ca_cert_dir) {
        (Some(_cert_dir), Some(_ca_dir)) => {
            let acceptor = tls::build_tls_acceptor(
                // It feels weird that you would match something, not use the reference, but also clone the value it references
                // When matching, use the value captured - it will help with reading&refactors.
                // Also, regarding the clone, you may be able to `take` the memory instead, if `config` no longer gets borrows
                // and instead gets owned transfers of values.
                config.cert_dir.clone().unwrap(),
                config.trusted_ca_cert_dir.clone().unwrap()
            );
            
            let listener = TcpListener::bind(config.bind_ip_tls.clone())
                .await
                .inspect_err(|err| {
                    println!("Binding to {} failed: {}", &config.bind_ip_tls, err);
                })?;
            
            Some(TlsListener {
                acceptor,
                tcp_listener: listener
            })
        }
        _ => None
    };
    
    // You can of course declare async functions mid funciton, but it has a few implications
    // 1) The function is now, at least logically, tied to the lifetime of the parent function.
    // 2) This does not play well with async usually, and probably means the parent function (main)
    //   cannot move between threads (i.e. it's not Send, aka `!Send`).
    // If it were me, I would declare this function elsewhere, below the main function.
    // I tend to declare non-async helper functions inside function bodies, for the purpose of
    // documentation and not leaking function behaviour (ex, `fn map_specific_field_to_specific_thing()`)
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

    // Loops are great, but it's good to have a clear termination condition
    loop {
        tokio::select! {
            plain = listener.accept() => {
                match plain {
                    // Often, it's worth doing a `tokio::spawn` for handling connections
                    Ok((socket, addr)) => process_socket_silent(
                        socket,
                        addr,
                        // If the config gets cloned often, it's good to put it in an `Arc`
                        config.clone(),
                        user_db.clone(),
                        queue.clone()
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
                        queue.clone(),
                    ).await,
                    Err(err) => eprintln!("Error accepting TLS stream: '{}'", err)
                }
            }
        }
    }
}

async fn process_socket_silent<T: IO>(stream: T, addr: SocketAddr, config: Config, user_db: UserDBMtx, queue: QueueMtx) {
    let handler = ConnectionHandler::new(stream, addr);
    debug!("Incoming client: {}:{}", handler.addr.ip(), handler.addr.port());
    match handler.server_loop(config, user_db, queue.clone()).await {
        Ok(()) => {
            queue.lock().await.fire().await;
    },
        Err(err) => eprintln!("Socket came back with error: '{err}'")
    }
    println!("Closing connection from {}:{}", addr.ip(), addr.port());
}
