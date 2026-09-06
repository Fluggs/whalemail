use std::{io, sync};
use std::time::Duration;
use multimap::MultiMap;
use rand::seq::SliceRandom;
use hickory_resolver::name_server::TokioConnectionProvider;
use hickory_resolver::{ResolveError, Resolver};
use log::debug;
use regex::Regex;
use rustls_pki_types::ServerName;
use tokio::net::TcpStream;
use tokio::time::timeout;
use tokio_rustls::client::TlsStream;
use crate::config::Config;
use crate::net::{ConnectionHandler, IO};
use crate::smtp::envelope::{Envelope, MailAddress};
use crate::smtp::server::StateKind;
use crate::tls::build_tls_connector;

#[derive(Debug)]
pub(crate) enum ClientError {
    DnsLookupError(ResolveError),
    IoError(io::Error),
    NoSmtpHostError
}

impl From<ResolveError> for ClientError {
    fn from(value: ResolveError) -> Self {
        ClientError::DnsLookupError(value)
    }
}

impl From<io::Error> for ClientError {
    fn from(value: io::Error) -> Self {
        ClientError::IoError(value)
    }
}

// Regex Patterns
struct Patterns {
    greeting: Regex,
}

static RE: sync::LazyLock<Patterns> = sync::LazyLock::new(|| Patterns {
    greeting: Regex::new(r"220 (\S+)\s*[^\r]*\r\n$").unwrap(),
});


pub(crate) enum Connection {
    TLS(GreetingState<TlsStream<TcpStream>>),
    TCP(GreetingState<TcpStream>)
}

struct GreetingState<T: IO> {
    remote_host: String,
    conn: ConnectionHandler<T>
}

impl<T: IO> GreetingState<T> {
    async fn new(mut conn: ConnectionHandler<T>) -> Result<Option<GreetingState<T>>, io::Error> {
        let s = conn.read().await?;
        let hostname = RE.greeting.captures(s.as_str())
            .and_then(
                |capture| capture.get(1)
                    .and_then(|host| Some(host.as_str().to_string()))
            );

        match hostname {
            Some(hostname) => {
                debug!("Recognized SMTP greeting: '{}'", s);
                Ok(Some(Self {
                    remote_host: hostname,
                    conn,
                }))
            },
            None => {
                debug!("Invalid SMTP greeting: '{}'", s);
                Ok(None)
            }
        }
    }
}

pub(crate) struct SmtpClient<T: IO> {
    config: Config,
    conn: ConnectionHandler<T>,
    dns_resolver: Resolver<TokioConnectionProvider>,
    remote_hosts: MultiMap<u16, String>
}

impl<T: IO> SmtpClient<T> {
    /**
    Creates an SMTP client with the mission to deliver an envelope to a recipient.
    */
    pub(crate) async fn discover_connection(config: &Config, recipient: MailAddress) -> Result<Connection, ClientError> {
        let dns_resolver = Resolver::builder_tokio()?.build();
        let mut remote_hosts = Self::lookup(&dns_resolver, recipient).await?;

        let mut connection: Option<Connection> = None;
        while let Some(host) = Self::pop_host(&mut remote_hosts) {
            match Self::connect(config, host).await? {
                Connection::TLS(tls) => {
                    debug!("Found SMTP at port 465");
                    connection = Some(Connection::TLS(tls));
                    break;
                }
                Connection::TCP(tcp) => {
                    debug!("Found SMTP at port 25");
                    connection = Some(Connection::TCP(tcp));
                    break;
                }
            }
        };
        
        Ok(match connection {
            Some(either) => either,
            None => return Err(ClientError::NoSmtpHostError)
        })
    }

    /**
    Performs MX DNS lookup for this mailaddress host and returns it as a multimap of the form
    preference -> exchange (which is DNS for priority -> host).
     */
    async fn lookup(resolver: &Resolver<TokioConnectionProvider>, recipient: MailAddress) -> Result<MultiMap<u16, String>, ResolveError> {
        let response = resolver.mx_lookup(recipient.domain.as_str()).await?;
        let mut lookup = MultiMap::new(); 
        response.iter().for_each(
            |mx| {
                debug!(target: "dns", "Adding MX host '{}' with prio '{}'", mx.exchange(), mx.preference());
                lookup.insert(mx.preference(), mx.exchange().to_string())
            }
        );
        
        let keys: Vec<_> = lookup.keys().map(|key| *key).collect();
        keys.into_iter().for_each(
            |key| lookup.get_vec_mut(&key).unwrap().shuffle(&mut rand::rng())
        );
        Ok(lookup)
    }
    
    /**
    Pops an MX host of the highest available priority (lowest value) and returns it.
    */
    fn pop_host(remote_hosts: &mut MultiMap<u16, String>) -> Option<String> {
        // Find lowest prio
        let mut prio = None;
        remote_hosts.keys().for_each(
            |key| if prio.is_none() || *key < prio.unwrap() {
                prio = Some(*key);
            }
        );
        
        let prio = match prio {
            Some(prio) => prio,
            None => return None
        };
        
        // Existence is proven by prior loops, therefore both .unwrap() are deemed infallible
        let host = remote_hosts.get_vec_mut(&prio).unwrap().pop().unwrap();
        
        if remote_hosts.get_vec(&prio).unwrap().is_empty() {
            remote_hosts.remove(&prio);
        }
        
        // None option is returned early
        Some(host)
    }
    
    /**
    Establishes a connection to a host.
    Tries implicit TLS (port 465) first, plaintext (port 25) second.
    */
    async fn connect(config: &Config, host: String) -> Result<Connection, ClientError> {
        debug!("Trying TLS");
        let conn_try = timeout(
            Duration::from_millis(10_000),
            Self::connect_tls(config, &host)
        ).await;
        if let Ok(Some(tls)) = conn_try {
            return Ok(Connection::TLS(tls));
        }

        debug!("Trying plaintext");
        let conn_try = timeout(
            Duration::from_millis(10_000),
            Self::connect_plaintext(&host)
        ).await;
        if let Ok(Some(plain)) = conn_try {
            return Ok(Connection::TCP(plain));
        }

        Err(ClientError::NoSmtpHostError)
    }

    async fn connect_tls(config: &Config, host: &String) -> Option<GreetingState<TlsStream<TcpStream>>>
    {
        let mut tls_host = host.clone();
        tls_host.push_str(":465");
        let conn = match ConnectionHandler::<TcpStream>::connect(&tls_host).await {
            Ok(conn) => conn,
            Err(ioerr) => {
                debug!("No SMTP at {}: '{:?}'", tls_host, ioerr);
                return None;
            }
        };

        // try TLS
        let tls = build_tls_connector(config);
        match tls.connect(ServerName::try_from(host.clone()).unwrap(), conn.socket).await {
            Ok(tls) => {
                let conn = ConnectionHandler::new(tls, conn.addr);
                match GreetingState::new(conn).await {
                    Ok(Some(greeting)) => Some(greeting),
                    Ok(None) => None,
                    Err(err) => {
                        debug!("No SMTP at {}: '{:?}'", tls_host, err);
                        None
                    }
                }
            },
            Err(err) => {
                debug!("No SMTP at {}: '{:?}'", tls_host, err);
                None
            }
        }
    }

    async fn connect_plaintext(host: &String) -> Option<GreetingState<TcpStream>> {
        // Try plaintext
        let mut plain_host = host.clone();
        plain_host.push_str(":25");
        let conn = match ConnectionHandler::<TcpStream>::connect(&plain_host).await {
            Ok(conn) => conn,
            Err(err) => {
                debug!("No SMTP at {}: '{:?}'", plain_host, err);
                return None
            }
        };

        match GreetingState::new(conn).await {
            Ok(Some(greeting)) => Some(greeting),
            Ok(None) => None,
            Err(err) => {
                debug!("No SMTP at {}: '{:?}'", plain_host, err);
                None
            }
        }
    }

    pub(crate) fn connhandler_mut(&mut self) -> &mut ConnectionHandler<T> {
        &mut self.conn
    }
    
    pub(crate) async fn step(mut self, input: String) -> Result<(Self, StateKind), io::Error> {
        todo!()
    }
}