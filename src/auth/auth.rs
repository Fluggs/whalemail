use std::io::Write;
use rsasl::callback::{Context, SessionCallback, SessionData};
use rsasl::prelude::*;
use rsasl::validate::{Validate, Validation, ValidationError};
use rsasl::config::SASLConfig;
use rsasl::mechanisms::*;
use rsasl::registry::{Mechanism, Registry};
use log::{debug, info};
use rsasl::property::{AuthId, AuthzId, Password};
use crate::auth::userdb::{UserDBMtx};

static MECHANISMS: &[Mechanism] = &[plain::PLAIN];

#[derive(Debug)]
pub enum Error {
    AuthUnsuccessful,
}

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

impl Write for Writer {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let buf = String::from_utf8_lossy(buf).to_string();
        let r = buf.len();
        debug!("SASL writing to client: {}", buf);
        self.write_buf = Some(buf);

        Ok(r)
    }

    fn flush(&mut self) -> std::io::Result<()> { Ok(()) }
}

pub struct Auth {
    session: Session<AuthValidation>,
    writer: Writer
}

impl Auth {
    pub(crate) fn new(user_db: UserDBMtx, selected: String) -> Auth {
        let selected = Mechname::parse(selected.as_ref()).unwrap();
        let callback = Callback{ user_db: user_db.clone() };
        let sasl = SASLConfig::builder()
            .with_registry(Registry::with_mechanisms(MECHANISMS))
            .with_callback(callback)
            .unwrap();
        Auth {
            session: SASLServer::<AuthValidation>::new(sasl).start_suggested(selected).unwrap(),
            writer: Writer{ write_buf: None }
        }
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
        let r = &self.session.step(input, &mut self.writer);
        
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
        debug!("Validation for user {} for identity {}", user.username, user.identity);
        if self.user_db.lock().unwrap().authorize(&user, password) {
            debug!("Authorizing user {} for identity {}", user.username, user.identity);
            validate.finalize::<AuthValidation>(Ok(user))
        }
        
        Ok(())
    }
}