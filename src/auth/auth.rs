use std::io::Write;
use rsasl::callback::{Context, SessionCallback, SessionData};
use rsasl::prelude::*;
use rsasl::validate::{Validate, Validation, ValidationError};
use rsasl::config::SASLConfig;
use rsasl::mechanisms::*;
use rsasl::registry::{Mechanism, Registry};
use log::{debug, info};
use rsasl::property::{AuthId, AuthzId, Password};
use tokio::io;
use crate::auth::userdb::{UserDBMtx};
use crate::net::IO;
use crate::smtp::smtp::ConnectionWriter;

static MECHANISMS: &[Mechanism] = &[plain::PLAIN, login::LOGIN];

#[derive(Debug)]
pub enum Error {
    AuthUnsuccessful,
    NoMechanism,
}

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
            Err(Error::NoMechanism)
        }
    }
}

/**
Represents an authenticated and authorized user/identity.
*/
pub struct Authorized {
    pub(crate) identity: String,
    username: String,
}

impl Authorized {
    fn new(identity: Option<&str>, username: Option<&str>, password: Option<&[u8]>) -> Result<(Authorized, String), Error> {
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
                identity,
                username
            },
           password
        ))
    }
}

pub struct AuthValidation;

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
                prefix.push_str(buf.as_str());
            }
            None => { }
        };
        prefix.push_str(suffix);
        conn.send(prefix).await.or_else(|e| Err(e.io_error.expect("Expected io error")))
    }
}

impl Write for Writer {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
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
    session: Session<AuthValidation>,
    writer: Writer,
    mechname: AuthMech,
}

impl Auth {
    pub(crate) fn new(user_db: UserDBMtx, selected: String) -> Result<Auth, Error> {
        let mechname = Mechname::parse(selected.as_ref()).unwrap();
        let callback = Callback{ user_db: user_db.clone() };
        let sasl = SASLConfig::builder()
            .with_registry(Registry::with_mechanisms(MECHANISMS))
            .with_callback(callback)
            .unwrap();
        let session = match SASLServer::<AuthValidation>
                ::new(sasl).start_suggested(mechname) {
            Ok(session) => session,
            Err(_) => return Err(Error::NoMechanism)
        };
        
        let mut r = Auth {
            session,
            writer: Writer { write_buf: None },
            mechname: AuthMech::from(mechname)?,
        };
        
        match &r.mechname {
            AuthMech::PLAIN => {}
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
      * Ok(Some(Authorized)) on successful auth
      * Ok(None) if the protocol has not finished yet
      * Err(Error.AuthUnsuccessful) if the auth process finished without auth success
    */
    pub(crate) fn step(&mut self, input: Option<&[u8]>) -> Result<Option<Authorized>, Error>{
        debug!("auth stepping {:?}", String::from_utf8_lossy(input.unwrap_or("<None>".as_bytes())));
        let r = match self.mechname {
            AuthMech::PLAIN => self.session.step(input, &mut self.writer),
            AuthMech::LOGIN => self.session.step64(input, &mut self.writer),
        };
        
        match r {
            Ok(State::Finished(MessageSent::No)) => {},
            Ok(State::Finished(MessageSent::Yes)) => {
                debug!("TODO! Sent SASL message");
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
                    Ok(authorized) => Ok(Some(authorized)),
                    Err(e) => {
                        debug!("Validation error: {:?}", e);
                        Err(Error::AuthUnsuccessful)
                    }
                }
            },
            None => Err(Error::AuthUnsuccessful)
        }
    }
    
    pub(crate) async fn flush<T: IO>(&mut self, conn_writer: &mut ConnectionWriter<T>, prefix: String, suffix: &str)
                              -> Result<(), io::Error>
    {
        self.writer.flush_to_connwriter(conn_writer, prefix, suffix).await
    }
}

struct Callback {
    user_db: UserDBMtx
}

impl SessionCallback for Callback {
    fn validate(&self, _session_data: &SessionData, context: &Context, validate: &mut Validate<'_>) -> Result<(), ValidationError> {
        let (user, password) = match Authorized::new(
                context.get_ref::<AuthzId>(),
                context.get_ref::<AuthId>(),
                context.get_ref::<Password>())
        {
            Ok((user, password)) => (user, password),
            Err(_) => return Ok(())
        };
        debug!("Validation for user {} for identity {} with pw {}", user.username, user.identity, password);
        if self.user_db.lock().unwrap().authorize(&user, password) {
            debug!("Authorizing user {} for identity {}", user.username, user.identity);
            validate.finalize::<AuthValidation>(Ok(user))
        }
        
        Ok(())
    }
}