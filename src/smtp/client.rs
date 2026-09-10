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
use crate::smtp::error::ClientError;
use crate::tls::build_tls_connector;

// Regex Patterns
struct Patterns {
    greeting: Regex,
}

static RE: sync::LazyLock<Patterns> = sync::LazyLock::new(|| Patterns {
    greeting: Regex::new(r"220 (\S+)\s*[^\r]*\r\n$").unwrap(),
});

#[allow(clippy::large_enum_variant)]
pub(crate) enum Connection {
    Tls(ConnectionHandler<TlsStream<TcpStream>>),
    Tcp(ConnectionHandler<TcpStream>),
}

impl Connection {
    async fn send(&mut self, msg: String) -> Result<(), io::Error> {
        match self {
            Connection::Tcp(tcp) => tcp.send(msg).await,
            Connection::Tls(tls) => tls.send(msg).await
        }
    }

    async fn read(&mut self) -> Result<String, io::Error> {
        match self {
            Connection::Tcp(tcp) => tcp.read().await,
            Connection::Tls(tls) => tls.read().await
        }
    }
}

enum SmtpState {
    Greeting(RemoteGreeting),
}

pub(crate) struct RemoteGreeting {
    remote_host: String
}

impl RemoteGreeting {
    /**
    Attempts to read a remote host's SMTP greeting. Returns:
    * `Ok(Some(RemoteGreeting))` when the remote host has sent a valid SMTP greeting
    * `Ok(None)` when the remote host has sent anything else or nothing at all
    * `Err(io::Error)` on IO error
    */
    async fn new<T: IO>(conn: &mut ConnectionHandler<T>) -> Result<Option<RemoteGreeting>, io::Error> {
        // todo handle remote sending nothing at all (maybe via tokio timeout)
        let s = conn.read().await?;
        let hostname = RE.greeting.captures(s.as_str())
            .and_then(
                |capture| capture.get(1)
                    .map(|host| host.as_str().to_string())
            );

        match hostname {
            Some(hostname) => {
                debug!("Recognized SMTP greeting: '{}'", s);
                Ok(Some(Self {
                    remote_host: hostname,
                }))
            },
            None => {
                debug!("Invalid SMTP greeting: '{}'", s);
                Ok(None)
            }
        }
    }
}

pub(crate) struct SmtpClient {
    conn: Connection,
    config: Config,
    envelope: Envelope,
    recipient: MailAddress,
    state: SmtpState,
}

impl SmtpClient {
    /**
    Creates an SMTP client with the mission to deliver an envelope to a recipient.
    */
    fn new(conn: Connection, config: Config, envelope: Envelope, recipient: MailAddress, greeting: RemoteGreeting) -> SmtpClient {
        SmtpClient {
            conn,
            config,
            envelope,
            recipient,
            state: SmtpState::Greeting(greeting),
        }
    }

    /**
    Delivers an envelope to a recipient.
    */
    pub(crate) async fn deliver(config: Config, envelope: Envelope, recipient: MailAddress) -> Result<(), ClientError>{
        let (conn, greeting) = SmtpClient::discover_connection(&config, &recipient).await?;
        let client = Self::new(conn, config, envelope, recipient, greeting);

        client.run().await
    }

    /**
    Builds the connection via which an envelope can be delivered to `recipient`.
    This involves looking up the DNS MX record an probing for SMTP ports.

    Returns the built connection and the initial SMTP client state in a `Result`.
    */
    pub(crate) async fn discover_connection(config: &Config, recipient: &MailAddress) -> Result<(Connection, RemoteGreeting), ClientError> {
        let dns_resolver = Resolver::builder_tokio()?.build();
        let mut remote_hosts = Self::lookup(&dns_resolver, recipient).await?;

        while let Some(host) = Self::pop_host(&mut remote_hosts) {
            if let Some(res) = Self::connect(config, host).await {
                return Ok(res)
            }
        }

        Err(ClientError::NoSmtpHostError)
    }

    /**
    Performs MX DNS lookup for this mailaddress host and returns it as a multimap of the form
    preference -> exchange (which is DNS for priority -> host).
     */
    async fn lookup(resolver: &Resolver<TokioConnectionProvider>, recipient: &MailAddress) -> Result<MultiMap<u16, String>, ResolveError> {
        let response = resolver.mx_lookup(recipient.domain.as_str()).await?;
        let mut lookup = MultiMap::new();
        response.iter().for_each(
            |mx| {
                debug!(target: "dns", "Adding MX host '{}' with prio '{}'", mx.exchange(), mx.preference());
                lookup.insert(mx.preference(), mx.exchange().to_string())
            }
        );

        let keys: Vec<_> = lookup.keys().copied().collect();
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

        let prio = prio?;

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

    Returns None when both attempts fail.
    */
    async fn connect(config: &Config, host: String) -> Option<(Connection, RemoteGreeting)> {
        debug!("Trying TLS");
        let conn_try = timeout(
            Duration::from_millis(10_000),
            Self::connect_tls(config, &host)
        ).await;
        if let Ok(Some(r)) = conn_try {
            debug!("Found SMTP at port 465");
            return Some(r);
        }

        debug!("Trying plaintext");
        let conn_try = timeout(
            Duration::from_millis(10_000),
            Self::connect_plaintext(host.as_str())
        ).await;
        if let Ok(Some(r)) = conn_try {
            debug!("Found SMTP at port 25");
            return Some(r);
        }

        None
    }

    /**
    Attempts to connect to the TLS port of a host and looks for an SMTP greeting there.
    */
    async fn connect_tls(config: &Config, host: &str) -> Option<(Connection, RemoteGreeting)>
    {
        let mut tls_host = host.to_string();
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
        match tls.connect(ServerName::try_from(host.to_string()).unwrap(), conn.socket).await {
            Ok(tls) => {
                let mut conn = ConnectionHandler::new(tls, conn.addr);
                match RemoteGreeting::new(&mut conn).await {
                    Ok(Some(greeting)) => Some((Connection::Tls(conn), greeting)),
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

    /**
    Attempts to connect to the regular SMTP port of a host and looks for an SMTP greeting there.
     */
    async fn connect_plaintext(host: &str) -> Option<(Connection, RemoteGreeting)> {
        // Try plaintext
        let mut plain_host = host.to_string();
        plain_host.push_str(":25");
        let mut conn = match ConnectionHandler::<TcpStream>::connect(&plain_host).await {
            Ok(conn) => conn,
            Err(err) => {
                debug!("No SMTP at {}: '{:?}'", plain_host, err);
                return None
            }
        };

        match RemoteGreeting::new(&mut conn).await {
            Ok(Some(greeting)) => Some((Connection::Tcp(conn), greeting)),
            Ok(None) => None,
            Err(err) => {
                debug!("No SMTP at {}: '{:?}'", plain_host, err);
                None
            }
        }
    }

    async fn expect(conn: &mut Connection, expected_prefix: &str) -> Result<(), ClientError> {
        let r = match timeout(Duration::from_millis(30_000), conn.read()).await {
            Ok(res) => res?,
            Err(_) => return Err(ClientError::Timeout),
        };

        match r.starts_with(expected_prefix) {
            true => Ok(()),
            false => Err(ClientError::SmtpError(r))
        }
    }

    pub(crate) async fn run(mut self) -> Result<(), ClientError> {
        self.conn.send(format!("EHLO {}\r\n", self.config.hostname.as_str()).to_string()).await?;
        Self::expect(&mut self.conn, "250").await?;
        self.conn.send(format!("MAIL FROM:<{}>\r\n", self.envelope.sender).to_string()).await?;
        Self::expect(&mut self.conn, "250").await?;

        for rcpt in &mut self.envelope.recipients {
            self.conn.send(format!("RCPT TO:<{}>\r\n", rcpt).to_string()).await?;
            Self::expect(&mut self.conn, "250").await?;
        }

        self.conn.send("DATA\r\n".to_string()).await?;
        Self::expect(&mut self.conn, "354").await?;
        self.conn.send(self.envelope.body).await?;
        Self::expect(&mut self.conn, "250").await?;

        // previous 250 acknowledged successful mail delivery, so here we ignore the result
        let _ = self.conn.send("QUIT\r\n".to_string()).await;

        Ok(())
    }
}