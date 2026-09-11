use std::io::Write;
use std::sync;
use base64::Engine;
use base64::prelude::BASE64_STANDARD;
use rsasl::callback::{Context, SessionCallback, SessionData};
use rsasl::prelude::*;
use rsasl::validate::{Validate, Validation, ValidationError};
use rsasl::config::SASLConfig;
use rsasl::mechanisms::*;
use rsasl::registry::{Mechanism, Registry};
use rsasl::property::{AuthId, AuthzId, Password};
use log::{debug, info};
use regex::Regex;
use tokio::io;
use crate::config::Hostname;
use crate::userdb::UserDBMtx;
use crate::net::IO;
use crate::smtp::server::ConnectionWriter;
use crate::user::User;

static MECHANISMS: &[Mechanism] = &[plain::PLAIN, login::LOGIN];

// Regex Patterns
struct Patterns {
    plain_auth_msg: Regex,
}

static RE: sync::LazyLock<Patterns> = sync::LazyLock::new(|| Patterns {
    // matches identity\0username\0password format
    plain_auth_msg: Regex::new(r"[^\x00]+\x00[^\x00]+\x00[^\x00]+$").unwrap(),
});

#[derive(Debug)]
pub enum Error {
    AuthUnsuccessful,
    InvalidMechanism,
}

/// Returned by `authorized()` if the SASL protocol has not finished yet.
#[derive(Debug)]
pub struct NotFinishedError {}

#[allow(clippy::upper_case_acronyms)]
pub enum AuthMech {
    PLAIN,
    LOGIN,
}

impl AuthMech {
    fn from(mechname: &Mechname) -> Result<AuthMech, Error> {
        //todo find some fancy iterator map solution
        if mechname.eq("PLAIN") {
            Ok(AuthMech::PLAIN)
        } else if mechname.eq("LOGIN") {
            Ok(AuthMech::LOGIN)
        } else {
            Err(Error::InvalidMechanism)
        }
    }
}

/**
Represents an authenticated and authorized user/identity.
*/
#[derive(Clone)]
#[derive(Debug)]
pub struct Authorized {
    pub(crate) hostname: Hostname,
    //todo identity should be a MailAddress
    pub(crate) identity: String,
    pub(crate) username: String,
}

impl Authorized {
    /**
    Builds an authorization input tuple (user, password) from optional input.
    Validates that at least a username is supplied and copies identity from it if necessary.
    
    Returns `Error::AuthUnsuccessful` when mandatory input (username, password) is missing or when
    identity and username are different.
    */
    fn new(hostname: Hostname, identity: Option<&str>, username: Option<&str>, password: Option<&[u8]>) -> Result<(Authorized, String), Error> {
        let (username, password): (String, String) = match (username, password) {
            (None, _) | (_, None) => {
                return Err(Error::AuthUnsuccessful);
            }
            (Some(username), Some(password)) => {
                let password = match String::from_utf8(Vec::from(password)) {
                    Ok(pw) => pw,
                    Err(_) => return Err(Error::AuthUnsuccessful)
                };
                (username.to_string(), password)
            }
        };
        
        let identity = match identity {
            Some(identity) if identity.trim() == "" => username.clone(),
            Some(identity) => match identity.eq(&username) {
                true => identity.to_string(),
                false => {
                    info!("Authorization as a different user is not supported.");
                    return Err(Error::AuthUnsuccessful);
                }
            },
            None => username.clone()
        };
        
        Ok((
            Authorized {
                hostname,
                identity,
                username
            },
           password
        ))
    }
}

struct AuthValidation;

impl Validation for AuthValidation {
    type Value = Result<Authorized, Error>;
}

struct Writer {
    write_buf: Option<String>,
}

impl Writer {
    /**
    If this Writer has a filled buffer, sends it via ConnectionWriter.
    If the buffer is not filled, does nothing and silently returns Ok(()).
    */
    async fn flush_to_connwriter<T: IO>(&mut self, conn: &mut ConnectionWriter<T>, mut prefix: String, suffix: &str) -> io::Result<()> {
        match self.write_buf.take() {
            Some(buf) => {
                debug!("Flushing buffer '{}'", buf);
                prefix.push_str(buf.as_str());
                prefix.push_str(suffix);
                Ok(conn.send(prefix).await?)
            }
            None => { Ok(()) }
        }
    }
}

impl Write for Writer {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let buf = String::from_utf8_lossy(buf);
        let r = buf.len();
        debug!("SASL writing to client: '{}'", buf);
        match self.write_buf {
            Some(_) => {
                self.write_buf.as_mut().unwrap().push_str(buf.as_ref());
            }
            None => {
                self.write_buf = Some(buf.to_string())
            }
        }

        Ok(r)
    }

    fn flush(&mut self) -> io::Result<()> { unimplemented!() }
}

pub struct Auth {
    /// SASL session this struct is wrapped around
    session: Session<AuthValidation>,
    
    /// Output writer for messages to be sent by SASL implementation
    writer: Writer,
    
    /// SASL mechanism
    mechname: AuthMech,
    
    /// Whether this SASL session has finished.
    is_finished: bool,
    
    /// Result of this SASL session. None if session unfinished or on unsuccessful auth.
    authorized: Option<Authorized>,
}

impl Auth {
    /**
    Constructs an SASL session for mechanism `selected`.
    Performs the first protocol `step` if an `initial_step` message is supplied (such as PLAIN credentials).
    Validates the incoming credentials against `user_db`.

    If `initial_step` leads to an immediate authorization, `authorized()` will return the result.
    */
    pub(crate) fn new(
        user_db: UserDBMtx,
        hostname: Hostname,
        selected: String,
        initial_step: Option<String>
    ) -> Result<Auth, Error> {
        debug!("Building Auth with mechanism '{selected}' and mech argument '{:?}'", initial_step);
        let mechname = Mechname::parse(selected.as_ref())
            .or(Err(Error::InvalidMechanism))?;
        let callback = Callback { hostname, user_db: user_db.clone() };
        let sasl = SASLConfig::builder()
            .with_registry(Registry::with_mechanisms(MECHANISMS))
            .with_callback(callback)
            .unwrap();
        let session = match SASLServer::<AuthValidation>
                ::new(sasl).start_suggested(mechname) {
            Ok(session) => session,
            Err(_) => return Err(Error::InvalidMechanism)
        };
        
        debug!("Are we first: '{}'", session.are_we_first());
        
        let mut r = Auth {
            session,
            writer: Writer { write_buf: None },
            mechname: AuthMech::from(mechname)?,
            is_finished: false,
            authorized: None,
        };
        
        match &r.mechname {
            AuthMech::PLAIN => {
                if let Some(arg) = initial_step {
                    let credentials = Self::try_base64_plain_credentials(arg);
                    r.step(Some(credentials.as_ref()))?;
                }
            }
            AuthMech::LOGIN => {
                debug!("Doing initial step for mech LOGIN");
                r.session.step64(None, &mut r.writer).expect("Expected state");
            }
        };
        
        Ok(r)
    }
    
    /**
    Performs a step in the auth protocol.
    
    Returns:
      * Ok(Some(User)) on successful auth
      * Ok(None) if the protocol has not finished yet
      * Err(Error.AuthUnsuccessful) if the auth process finished without auth success
    */
    pub(crate) fn step(&mut self, input: Option<&[u8]>) -> Result<Option<User>, Error>{
        debug!("auth stepping {:?}", String::from_utf8_lossy(input.unwrap_or("<None>".as_bytes())));
        let r = match self.mechname {
            AuthMech::PLAIN => self.session.step(input, &mut self.writer),
            AuthMech::LOGIN => self.session.step64(input, &mut self.writer),
        };
        
        match r {
            Ok(State::Finished(MessageSent::No)) => {
                self.is_finished = true;
            },
            Ok(State::Finished(MessageSent::Yes)) => {
                debug!("TODO! Sent SASL message");
                self.is_finished = true;
                return Err(Error::AuthUnsuccessful)
            },
            Ok(State::Running) => {
                debug!("Auth step done, keep running");
                return Ok(None)
            },
            Err(e) => {
                debug!("SASL session error: {:?}", e);
                return Err(Error::AuthUnsuccessful) }
        }
        
        match self.session.validation() {
            Some(val) => {
                match val {
                    Ok(authorized) => {
                        let r = authorized.clone().into();
                        self.authorized = Some(authorized);
                        Ok(Some(r))
                    },
                    Err(e) => {
                        debug!("Validation error: {:?}", e);
                        Err(Error::AuthUnsuccessful)
                    }
                }
            },
            None => Err(Error::AuthUnsuccessful)
        }
    }

    /**
    Flushes the SASL write buffer to `conn_writer`; prefixes it with `prefix` and suffixes it with `suffix`.
    The write buffer is filled by SASL in case it wants to write something back.
    */
    pub(crate) async fn flush<T: IO>(&mut self, conn_writer: &mut ConnectionWriter<T>, prefix: String, suffix: &str)
                              -> Result<(), io::Error>
    {
        debug!("flushing");
        let r = self.writer.flush_to_connwriter(conn_writer, prefix, suffix).await;

        debug!("flushing done");
        r
    }

    /**
    Returns:
      * Ok(Some(Authorized)) on successful auth
      * Err(NotFinishedError) if the protocol has not finished yet
      * Ok(None) if the auth process finished without auth success
    */
    pub(crate) fn authorized(&self) -> Result<Option<User>, NotFinishedError> {
        if !self.is_finished {
            return Err(NotFinishedError{})
        }
        Ok(self.authorized.clone().map(|a| a.into()))
    }
    
    /**
    If PLAIN input `s` does not contain 2 x00 bytes, decodes PLAIN input `s` via base64 if possible.
    Always returns either `s` or base64-decoded `s`.
    */
    fn try_base64_plain_credentials(s: String) -> String {
        match RE.plain_auth_msg.is_match(&s) {
            true => s,
            false => match BASE64_STANDARD.decode(&s) {
                Ok(decoded) => String::from_utf8(decoded).unwrap_or(s),
                Err(_) => s,
            }
        }
    }
}

struct Callback {
    user_db: UserDBMtx,
    hostname: Hostname,
}

impl SessionCallback for Callback {
    fn validate(&self, _session_data: &SessionData, context: &Context, validate: &mut Validate<'_>) -> Result<(), ValidationError> {
        let (user, password) = match Authorized::new(
            self.hostname.clone(),
            context.get_ref::<AuthzId>(),
            context.get_ref::<AuthId>(),
            context.get_ref::<Password>()
        ) {
            Ok((user, password)) => (user, password),
            Err(_) => return Ok(())
        };
        debug!("Validation for user {} for identity {} with pw {}", user.username, user.identity, password);
        if self.user_db.lock().unwrap().authenticate(&user, password)
            .map_err(|dberr| ValidationError::Boxed(Box::new(dberr)))? {
            debug!("Authorizing user {} for identity {}", user.username, user.identity);
            validate.finalize::<AuthValidation>(Ok(user))
        }
        
        Ok(())
    }
}
