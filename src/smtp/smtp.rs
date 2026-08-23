use std::{fmt, io, mem, sync};
use std::fmt::{Debug, Formatter};
use strum::{Display, EnumString};
use strum_macros::{AsStaticStr, IntoStaticStr};
use log::{debug, info};
use regex::Regex;
use crate::config::Config;
use crate::auth::auth;
use crate::userdb::userdb::UserDBMtx;
use crate::net::{ConnectionHandler, IO};
use crate::tests::test::SmtpTest;
use crate::smtp::smtp_error::{DeliveryError, ErrorKind, SmtpError};
use crate::smtp::smtp_mail::{Envelope, MailAddress};
use crate::maildir::Storage;
use crate::user::User;

#[derive(Debug, Clone, PartialEq, Display, EnumString, IntoStaticStr)]
pub(crate) enum SmtpState {
    INIT,
    HELO,
    EHLO,
    MAIL,
    RCPT,
    DATA,
    DATAINPUT,
    QUIT,
    AUTH,
}

#[derive(Clone, Debug)]
#[derive(PartialEq)]
pub enum StateKind {
    CONTINUE,
    QUIT
}

#[derive(Clone)]
pub(crate) struct Command {
    pub(crate) verb: Option<SmtpState>,
    pub(crate) message: String,
}

impl fmt::Display for Command {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl Command {
    /**
    Builds a new Command from a client message.
    Tries to parse the first word as an SMTP command. If none is recognized, sets it to None.
    */
    fn new(s: String) -> Command {
        let mut split = s.trim().split(" ");
        let verb = match split.next() {
            Some(first) => first.parse::<SmtpState>().ok(),
            None => None
        };

        Command {
            verb,
            message: s,
        }
    }

    /**
    Returns a copy of Command.message with trailing \r\n removed.
    */
    fn msg_strip_lf(&self) -> Result<String, SmtpError> {
        let r = match self.message.ends_with("\r\n") {
            true => Ok(self.message[0..self.message.len() - 2].to_string()),
            false => Err(SmtpError::bad_command(self))
        };
        debug!("Turning '{:?}' into '{:?}'", self.message, r);
        r
    }
}

// Regex Patterns
struct Patterns {
    mail_end: Regex,
    period_linestart: Regex,
    auth_cmd: Regex,
}

static RE: sync::LazyLock<Patterns> = sync::LazyLock::new(|| Patterns {
    mail_end: Regex::new(r"\r\n\.\r\n").unwrap(),
    period_linestart: Regex::new(r"\r\n\.").unwrap(),
    auth_cmd: Regex::new(r"AUTH ([0-9A-Za-z_-]*)\s*([^$]+?)?\s*$").unwrap(),
});

pub struct ConnectionWriter<T: IO> {
    pub(crate) conn: Option<ConnectionHandler<T>>,
    pub(crate) conn_testbed: Option<SmtpTest>,
}

impl<T: IO> ConnectionWriter<T> {
    pub(crate) async fn send(&mut self, s: String) -> Result<(), SmtpError> {
        match &mut self.conn {
            Some(c) => c.send(s).await
                .or_else(|error| Err(SmtpError::from_io(error))),
            None => {
                debug!("Sending test message '{:?}'", s);
                let mut r = Ok(());
                if cfg!(test) {
                    r = self.conn_testbed.as_mut().unwrap().send(s);
                } else if cfg!(not(test)) {
                    panic!("No connection handler present");
                }
                r
            },
        }
    }
}

pub trait SmtpStateT<State: Send>: Sized {
    async fn from(state: State, cmd: Command) -> Result<Self, SmtpError>;
}

#[derive(Debug)]
struct InitState {
}

impl InitState {
    pub(crate) async fn greet<T: IO>(writer: &mut ConnectionWriter<T>, _config: &Config) -> Result<Self, SmtpError> {
        // todo refine greeting message
        writer.send("220 hi\r\n".to_string()).await?;
        Ok(Self {})
    }
}

#[derive(Debug)]
struct HeloState {
}

impl HeloState {
    async fn respond<T: IO>(writer: &mut ConnectionWriter<T>, _from: InitState) -> Result<Self, SmtpError> {
        writer.send("250 OK\r\n".to_string()).await?;
        Ok(Self {})
    }
    
    async fn mail_from<T: IO>(
        self,
        writer: &mut ConnectionWriter<T>,
        cmd: Command
    ) -> Result<MailState, SmtpError> {
        MailState::mail_from(writer, cmd, SmtpState::HELO).await
    }
}

#[derive(Debug)]
struct EhloState {
}

impl EhloState {
    fn build_ehlo_response(config: &Config) -> String {
        format!(
            "250-{}\r\n\
            250 AUTH PLAIN LOGIN\r\n"
            , config.hostname
        ).to_string()
    }
    async fn respond<T: IO>(writer: &mut ConnectionWriter<T>, config: &Config, _from: InitState) -> Result<Self, SmtpError> {
        writer.send(EhloState::build_ehlo_response(config)).await?;
        Ok(Self {})
    }

    async fn mail_from<T: IO>(
        self,
        writer: &mut ConnectionWriter<T>,
        cmd: Command
    ) -> Result<MailState, SmtpError> {
        MailState::mail_from(writer, cmd, SmtpState::EHLO).await
    }
}

#[cfg(test)]
pub(crate) fn ehlo_response(config: &Config) -> String {
    EhloState::build_ehlo_response(config)
}

struct AuthState {
    auth: auth::Auth,
    user: Option<User>,
    from_state: EhloState,
}

impl AuthState {
    /**
    Handles an AUTH command. Starts the SASL auth process.
    */
    async fn init_sasl<T: IO>(
        writer: &mut ConnectionWriter<T>,
        cmd: Command,
        user_db: UserDBMtx,
        from: EhloState
    ) -> Result<SmtpState2, SmtpError> {
        // Parse AUTH <mech> [mech_arg]
        let (mech, mech_arg) = match RE.auth_cmd.captures(cmd.message.as_str()) {
            Some(v) => (v.get(1), v.get(2)),
            None => (None, None)
        };

        let mech = match mech {
            Some(re_match) => String::from(re_match.as_str())
                .to_ascii_uppercase(),
            None => return Err(SmtpError::bad_auth_mech(&cmd, SmtpState::EHLO))
        };

        let mech_arg = mech_arg
            .and_then(|re_match| Some(String::from(re_match.as_str())));

        let auth = match auth::Auth::new(user_db.clone(), mech, mech_arg) {
            Ok(auth) => auth,
            Err(auth::Error::InvalidMechanism) => return Err(SmtpError::bad_auth_mech(&cmd, SmtpState::EHLO)),
            Err(auth::Error::AuthUnsuccessful) => return Err(SmtpError::bad_credentials(&cmd, SmtpState::EHLO))
        };
        
        let mut auth_state = Self {
            auth,
            user: None,
            from_state: from
        };
        
        auth_state.auth_flush(writer).await?;
        debug!("authorized: '{:?}'", auth_state.auth.authorized());

        match auth_state.auth.authorized() {
            Ok(Some(authorized)) => {
                auth_state.finalize_authorization(writer, authorized).await?;
                Ok(SmtpState2::EHLO(auth_state.from_state))
            },
            Ok(None) => {
                Err(SmtpError::bad_credentials(&cmd, SmtpState::EHLO))?
            },

            // SASL not finished yet, keep going
            Err(_) => { Ok(SmtpState2::AUTH(auth_state)) }
        }
    }

    /**
    Handles AUTH (SASL) steps once this protocol is in AUTH state.
    Returns to EHLO state once the SASL process is done (with or without successful auth).
    */
    async fn handle_auth_step<T: IO>(
        mut self,
        writer: &mut ConnectionWriter<T>,
        cmd: Command
    ) -> Result<SmtpState2, SmtpError> {
        match self.auth.step(Some(cmd.msg_strip_lf()?.as_ref())) {
            Ok(None) => {
                self.auth_flush(writer).await?;
                Ok(SmtpState2::AUTH(self))
            },
            Ok(Some(user)) => {
                self.finalize_authorization(writer, user).await
                    .and(Ok(SmtpState2::EHLO(self.from_state)))
            },
            Err(_) => {
                Err(SmtpError::bad_credentials(&cmd, SmtpState::AUTH))
            }
        }
    }
    
    /**
    Sends success message and handles Smtp state for an authorization success.
    */
    async fn finalize_authorization<T: IO>(
        &mut self,
        writer: &mut ConnectionWriter<T>,
        user: User
    ) -> Result<(), SmtpError> {
        debug!("Finalizing auth");
        self.user = Some(user);
        writer.send("235 2.7.0 Authentication successful\r\n".to_string())
            .await
            .and(Ok(()))
    }

    /**
    Flushes the write buffer of `self.auth` in case SASL wants to write something.
    */
    async fn auth_flush<T: IO>(&mut self, cw: &mut ConnectionWriter<T>) -> Result<(), SmtpError> {
        self.auth
            .flush(cw, "334 ".to_string(), "\r\n")
            .await
            .or_else(|e| Err(SmtpError::from_io(e)))
    }
}

impl Debug for AuthState {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "AuthState")
    }
}

/**
Parses an address from an RCPT or MAIL command.
Returns the contained address or `SmtpError::BADCOMMAND` on error.
*/
fn parse_address_message_by_re(re: Regex, cmd: &Command) -> Result<String, SmtpError> {
    let parse = match re.captures(&cmd.message) {
        Some(capture) => match capture.get(1) {
            Some(rcpt) => Some(rcpt.as_str().to_string()),
            None => None
        },
        None => None
    };

    match parse {
        Some(rcpt) => Ok(rcpt),
        None => Err(SmtpError::bad_command(&cmd))
    }
}

struct MailState {
    sender: String,
}

impl MailState {
    async fn mail_from<T: IO>(
        writer: &mut ConnectionWriter<T>,
        cmd: Command,
        old_state: SmtpState
    ) -> Result<Self, SmtpError> {
        debug!("Handling MAIL: {}", cmd);
        let sender = parse_address_message_by_re(
            Regex::new(r"^MAIL FROM:<([^>]+)>\r\n$").unwrap(), &cmd
        )?;
        writer.send("250 OK\r\n".to_string()).await?;
        Ok(Self {
            sender,
        })
    }
}

struct RcptState {
    sender: String,
    recipients: Vec<MailAddress>
}

impl RcptState {
    async fn new<T: IO>(
        writer: &mut ConnectionWriter<T>,
        cmd: Command,
        old_state: MailState
    ) -> Result<Self, SmtpError> {
        let mut r = Self {
            sender: old_state.sender,
            recipients: Vec::new()
        };
        
        r.add_rcpt(writer, cmd).await?;
        Ok(r)
    }
    
    async fn add_rcpt<T: IO>(&mut self, writer: &mut ConnectionWriter<T>, cmd: Command)
        -> Result<(), SmtpError> {
        let recipient = parse_address_message_by_re(
            Regex::new(r"^RCPT TO:<([^>]+)>\r\n$").unwrap(), &cmd
        )?;
        let recipient = MailAddress::new(recipient.as_str())
            // todo remove state arg from SmtpError
            .map_err(|_| SmtpError::invalid_mailbox(&cmd, SmtpState::MAIL))?;

        self.recipients.push(recipient);
        writer.send("250 OK\r\n".to_string()).await?;
        Ok(())
        
    }
}

struct DataState {
    user_db: UserDBMtx,
    sender: String,
    recipients: Vec<MailAddress>,
    mail_body: String,
    body_finished: bool,
}

impl DataState {
    async fn new<T: IO>(
        writer: &mut ConnectionWriter<T>,
        user_db: UserDBMtx,
        old_state: RcptState)
        -> Result<Self, SmtpError> {
        writer.send("354 start mail input\r\n".to_string()).await?;
        
        Ok(Self {
            user_db,
            sender: old_state.sender,
            recipients: old_state.recipients,
            mail_body: String::new(),
            body_finished: false,
        })
    }
    
    #[cfg(test)]
    fn mock(user_db: UserDBMtx) -> Self {
        Self {
            user_db,
            sender: String::new(),
            recipients: Vec::new(),
            mail_body: String::new(),
            body_finished: false,
        }   
    }
    
    async fn receive_data<T: IO>(
        mut self,
        writer: &mut ConnectionWriter<T>,
        storage: &mut Storage,
        cmd: Command) 
    -> Result<SmtpState2, SmtpError> {
        debug!("Mail!: {}", cmd.message);
        let mail_end = self.decode_transparency(cmd.message);
        match mail_end {
            true => {
                self.body_finished = true;
                let mail = Envelope::new(self.sender, self.recipients, self.mail_body);
                match Self::deliver_mail(storage, self.user_db.clone(), &mail).await {
                    Ok(()) => {
                        writer.send("250 OK\r\n".to_string()).await?;
                        Ok(SmtpState2::DATACOMPLETE(CompleteState::new(mail)))
                    },
                    Err(err) => Err(err.into())
                }
            },
            false => Ok(SmtpState2::DATA(self))
        }
    }

    /**
    Attempts to deliver a mail to its recipients. Returns true only if the mail could be delivered
    to all recipients.
    */
    async fn deliver_mail(storage: &Storage, user_db: UserDBMtx, envelope: &Envelope) -> Result<(), DeliveryError> {
        let mut mailboxes: Vec<(&MailAddress, String)> = Vec::new();
        for rcpt in &envelope.recipients {
            let mb = match user_db.lock().unwrap().get_mailboxhome(rcpt) {
                Ok(mb) => mb,
                Err(_) => return Err(DeliveryError::NoSuchUser(rcpt.address.clone()))
            };
            mailboxes.push((rcpt, mb));
        }

        for (rcpt, mb) in mailboxes {
            match storage.store(envelope, rcpt, mb.clone()).await {
                Ok(()) => {},
                Err(err) => {
                    debug!("Error storing mail for '{:?}': '{}'", mb, err);
                    return Err(DeliveryError::MailboxIO(format!("{}", err)));
                }
            }
        }

        Ok(())
    }
    
    /**
    Decodes mails as per transparency procedure in RFC5321#4.5.2 and pushes the result to Smtp.mail.
    */
    pub(crate) fn decode_transparency(&mut self, s: String) -> bool {
        debug!("Decoding transparency for \"{s}\"");
        let mut buf: Vec<&str> = Vec::new();
        let mut capacity = 0;
        let mut has_changed = false;

        // Catch empty mail; treat period on first line as end of mail
        match s.starts_with(".\r\n") {
            true => {
                self.mail_body = ".\r\n".to_string();
                return true;
            }
            false => {}
        }

        // Find and store \r\n.\r\n positions
        let (mail_end_start, mail_end_end, mail_is_complete) = match RE.mail_end.find(&s) {
            Some(m) => (m.start(), m.end(), true),
            None => (s.len(), s.len(), false)
        };


        // Handle transparency on first line
        let mut chunk_start = match s.starts_with(".") {
            true => {
                has_changed = true;
                1
            },
            false => 0
        };

        // Write every chunk between two \r\n. into buf to assemble the new body later
        let mut end_loop = false;
        while !end_loop {

            let chunk_end = match RE.period_linestart.find(&s[chunk_start..mail_end_start]) {
                Some(m) => {
                    has_changed = true;
                    m.start() + "\r\n".len() + 1
                },
                None => {
                    end_loop = true;
                    mail_end_end
                }
            };

            buf.push(&s[chunk_start..chunk_end]);
            capacity += chunk_end - chunk_start;
            debug!("Recognized mail part with len {}:\n{:?}", chunk_end - chunk_start, &s[chunk_start..chunk_end]);

            chunk_start = chunk_end + ".".len();
        }

        match has_changed {
            true => {
                self.mail_body.reserve(capacity);
                for el in buf {
                    self.mail_body += el;
                }
            },
            false => self.mail_body.push_str(s.as_str())
        };

        mail_is_complete
    }

    #[cfg(test)]
    pub(crate) fn is_finished(&self) -> bool { self.body_finished }
    
    #[cfg(test)]
    pub(crate) fn mail_body(&self) -> String {
        self.mail_body.clone()
    }
}

struct CompleteState {
    mail: Envelope
}

impl CompleteState {
    fn new(mail: Envelope) -> Self {
        Self {
            mail,
        }
    }
    
    async fn quit<T: IO>(&self, writer: &mut ConnectionWriter<T>) {
        let _ = writer.send("221 closing channel\r\n".to_string()).await;
    }
    
    #[cfg(test)]
    fn mail(&self) -> &Envelope {
        &self.mail
    }
}

#[derive(IntoStaticStr)]
enum SmtpState2 {
    // todo rename enum
    INIT(InitState),
    HELO(HeloState),
    EHLO(EhloState),
    AUTH(AuthState),
    MAIL(MailState),
    RCPT(RcptState),
    DATA(DataState),
    DATACOMPLETE(CompleteState),
    QUIT(CompleteState),
    CANCELLED,
}

impl Debug for SmtpState2 {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let s: &str = self.into();
        write!(f, "{}", s)
    }
}

pub(crate) struct Smtp2<T: IO> {
    // todo rename struct
    conn_writer: ConnectionWriter<T>,
    config: Config,
    state: SmtpState2,
    state_history: Vec<SmtpState2>,
    user_db: UserDBMtx,
    storage: Storage,
    authorized: Option<User>,
}

impl<T: IO> Smtp2<T> {
    async fn build(mut cw: ConnectionWriter<T>, config: Config, user_db: UserDBMtx, storage: Storage) -> Result<Self, SmtpError> {
        let state = InitState::greet(&mut cw, &config).await?;

        Ok(Self {
            conn_writer: cw,
            config,
            state: SmtpState2::INIT(state),
            state_history: vec![],
            user_db,
            storage,
            authorized: None,
        })
    }
    
    pub(crate) async fn new(
        connhandler: ConnectionHandler<T>,
        config: Config,
        user_db: UserDBMtx,
        storage: Storage
    ) -> Result<Self, SmtpError> {
        let cw = ConnectionWriter {
            conn: Some(connhandler),
            conn_testbed: None,
        };
        
        Self::build(cw, config, user_db, storage).await
    }
    
    async fn send(&mut self, s: String) -> Result<(), SmtpError> {
        self.conn_writer.send(s).await
    }

    pub(crate) fn connhandler_mut(&mut self) -> &mut ConnectionHandler<T> {
        let r = &mut self.conn_writer.conn;
        r.as_mut().expect("Unexpected test mode for SMTP connection handler")
    }
    
    pub(crate) async fn handle(mut self, input: String) -> Result<(Self, StateKind), io::Error> {
        let cmd = Command::new(input);

        debug!("Command: {}", match &cmd.verb {
            Some(v) => format!("{v}"),
            None => "<data input>".to_string()
        });
        
        self.state = match (self.state, &cmd.verb) {
            (SmtpState2::INIT(old_state), Some(SmtpState::HELO)) => {
                HeloState::respond(&mut self.conn_writer, old_state)
                    .await
                    .and_then(|helo| Ok(SmtpState2::HELO(helo)))
            },
            (SmtpState2::INIT(old_state), Some(SmtpState::EHLO)) => {
                EhloState::respond(&mut self.conn_writer, &self.config, old_state)
                    .await
                    .and_then(|ehlo| Ok(SmtpState2::EHLO(ehlo)))
            },
            (SmtpState2::EHLO(ehlo), Some(SmtpState::AUTH)) => {
                AuthState::init_sasl(
                    &mut self.conn_writer, cmd, self.user_db.clone(), ehlo
                ).await
            },
            (SmtpState2::AUTH(auth), _) => {
                auth.handle_auth_step(&mut self.conn_writer, cmd).await
            }
            (SmtpState2::HELO(helo), Some(SmtpState::MAIL)) => {
                helo.mail_from(&mut self.conn_writer, cmd)
                    .await
                    .and_then(|mail| Ok(SmtpState2::MAIL(mail)))
            },
            (SmtpState2::EHLO(ehlo), Some(SmtpState::MAIL)) => {
                ehlo.mail_from(&mut self.conn_writer, cmd)
                    .await
                    .and_then(|mail| Ok(SmtpState2::MAIL(mail)))
            },
            (SmtpState2::MAIL(mail), Some(SmtpState::RCPT)) => {
                RcptState::new(&mut self.conn_writer, cmd, mail)
                    .await
                    .and_then(|rcpt| Ok(SmtpState2::RCPT(rcpt)))
            },
            (SmtpState2::RCPT(mut rcpt), Some(SmtpState::RCPT)) => {
                rcpt.add_rcpt(&mut self.conn_writer, cmd)
                    .await
                    .and(Ok(SmtpState2::RCPT(rcpt)))
            },
            (SmtpState2::RCPT(rcpt), Some(SmtpState::DATA)) => {
                DataState::new(&mut self.conn_writer, self.user_db.clone(), rcpt)
                    .await
                    .and_then(|state| Ok(SmtpState2::DATA(state)))
            },
            (SmtpState2::DATA(state), _) => {
                state.receive_data(&mut self.conn_writer, &mut self.storage, cmd).await
            },
            (SmtpState2::DATACOMPLETE(state), Some(SmtpState::QUIT)) => {
                state.quit(&mut self.conn_writer).await;
                Ok(SmtpState2::QUIT(state))
            },
            (_state, _verb) => {
                debug!("Bad sequence: {:?}", self.state_history);
                Ok(self.conn_writer.send("503 Bad sequence\r\n".to_string()).await
                    .and(Ok(SmtpState2::CANCELLED))
                    .or_else(|err| Err(err.io_error.unwrap()))?)
            }
        }
            // todo handle error
            .unwrap();
        
        match self.state {
            SmtpState2::QUIT(_) | SmtpState2::CANCELLED => Ok((self, StateKind::QUIT)),
            _ => Ok((self, StateKind::CONTINUE))
        }
    }

    #[cfg(test)]
    pub(crate) async fn new_testbed (testbed: SmtpTest, config: Config, user_db: UserDBMtx, storage: Storage) -> Result<Self, SmtpError> {
        let cw = ConnectionWriter {
            conn: None,
            conn_testbed: Some(testbed),
        };

        Self::build(cw, config, user_db, storage).await
    }

    #[cfg(test)]
    pub(crate) fn get_testbed(&mut self) -> &SmtpTest {
        self.conn_writer.conn_testbed.as_mut().unwrap()
    }

    #[cfg(test)]
    pub(crate) fn receive(&mut self) -> Option<String> {
        self.conn_writer.conn_testbed.as_mut().unwrap().receive()
    }

    #[cfg(test)]
    pub(crate) fn expect_no_msg(&mut self) {
        self.conn_writer.conn_testbed.as_mut().unwrap().expect_no_msg()
    }

    #[cfg(test)]
    pub(crate) fn user_db(&self) -> &UserDBMtx {
        &self.user_db
    }

    #[cfg(test)]
    pub(crate) fn mail(&self) -> &Envelope {
        match &self.state {
            SmtpState2::DATACOMPLETE(complete)
            | SmtpState2::QUIT(complete) => {
                complete.mail()
            },
            _ => panic!("Incorrect smtp state: {:?}", self.state)
        }
    }

    #[cfg(test)]
    pub(crate) fn config(&self) -> &Config {
        &self.config
    }
    
    #[cfg(test)]
    pub(crate) fn decode_transparency(&self, s: String) -> bool {
        let mut data = DataState::mock(self.user_db.clone());
        data.decode_transparency(s)
    }
}

//#############################################################################################

pub struct Smtp<T: IO> {
    pub(crate) conn_writer: ConnectionWriter<T>,
    pub(crate) closed: bool,
    
    pub(crate) state: SmtpState,
    pub(crate) user: Option<User>,
    state_history: Vec<SmtpState>,
    pub(crate) mail: Envelope,

    config: Config,
    pub(crate) user_db: UserDBMtx,
    storage: Storage,
    
    auth: Option<auth::Auth>,
}

impl<T: IO> Smtp<T> {
    pub fn new(connhandler: ConnectionHandler<T>, config: Config, user_db: UserDBMtx, storage: Storage) -> Smtp<T> {
        Smtp {
            conn_writer: ConnectionWriter {
                conn: Some(connhandler),
                conn_testbed: None,
            },
            closed: false,
            state: SmtpState::INIT,
            state_history: Vec::new(),
            mail: Envelope::new2(),
            config,
            user_db,
            storage,
            auth: None,
            user: None,
        }
    }
    #[cfg(test)]
    pub fn new_testbed (testbed: SmtpTest, config: Config, user_db: UserDBMtx, storage: Storage) -> Smtp<T> {
        Smtp {
            conn_writer: ConnectionWriter {
                conn: None,
                conn_testbed: Some(testbed),
            },
            closed: false,
            state: SmtpState::INIT,
            state_history: Vec::new(),
            mail: Envelope::new2(),
            config,
            user_db,
            storage,
            auth: None,
            user: None,
        }
    }
    
    #[cfg(test)]
    pub(crate) fn config_ref(&self) -> &Config {
        &self.config
    }
    
    pub(crate) fn connhandler_mut(&mut self) -> &mut ConnectionHandler<T> {
        let r = &mut self.conn_writer.conn;
        r.as_mut().expect("Unexpected test mode for SMTP connection handler")
    }
    
    async fn send(&mut self, s: String) -> Result<(), SmtpError> {
        self.conn_writer.send(s).await
    }
    
    pub async fn init_smtp(&mut self) -> Result<(), SmtpError> {
        self.send("220 hi\r\n".to_string()).await?;
        self.state = SmtpState::INIT;
        
        Ok(())
    }
    
    /**
    State machine for SMTP.
    Builds a Command struct from the incoming message and calls handling functions.
    The handling function is chosen by current state, not by incoming command.
    Each handling function handles the command and returns the next state.

    Returns a StateKind that states whether the state machine is at an end or not.
    */
    pub async fn handle(&mut self, input: String) -> Result<StateKind, io::Error> {
        let cmd = Command::new(input);

        debug!("Command: {}", match &cmd.verb {
            Some(v) => format!("{v}"),
            None => "<data input>".to_string()
        });

        // state machine edge self.state -> cmd.verb
        let r = match &cmd.verb {
            // INIT -> HELO
            Some(SmtpState::HELO) => {
                self.handle_static_cmd(&cmd, vec![SmtpState::INIT], "250 OK\r\n")
                    .await
                    .and(Ok(SmtpState::HELO))
            },
            // INIT -> EHLO
            Some(SmtpState::EHLO) => {
                self.handle_ehlo(cmd)
                    .await
                    .and(Ok(SmtpState::EHLO))
            },
            // EHLO -> AUTH
            Some(SmtpState::AUTH) => {
                self.handle_auth(cmd).await
            }
            // HELO|EHLO -> MAIL
            Some(SmtpState::MAIL) => {
                self.receive_mail_cmd(&cmd).await
                    .and(Ok(SmtpState::MAIL))
            },
            // MAIL|RCPT -> RCPT
            Some(SmtpState::RCPT) => {
                self.receive_rcpt(&cmd).await
                    .and(Ok(SmtpState::RCPT))
            },
            // RCPT -> DATA
            Some(SmtpState::DATA) => {
                self.handle_static_cmd(&cmd, vec![SmtpState::RCPT], "354 start mail input\r\n")
                    .await
                    .and(Ok(SmtpState::DATA))
            },
            // DATA -> DATAINPUT
            Some(SmtpState::DATAINPUT) => {
                self.receive_data(cmd).await
            },
            // DATAINPUT -> QUIT
            Some(SmtpState::QUIT) => {
                self.handle_static_cmd(&cmd, vec![SmtpState::DATAINPUT], "221 closing channel\r\n")
                    .await
                    .and(Ok(SmtpState::QUIT))
            },
            None => {
                debug!("Handling None");
                match &self.state {
                    SmtpState::DATA => {
                        self.receive_data(cmd).await
                    },
                    SmtpState::AUTH => {
                        debug!("Auth step");
                        self.handle_auth_step(&cmd).await
                    },
                    _ => {
                        info!("Unrecognized SMTP message: \"{}\"", cmd.message);
                        Err(SmtpError::bad_command(&cmd))
                    }
                }
            },
            y => panic!("Unexpected {:?}", y)
        };
        
        self.state_history.push(self.state.clone());
        
        match r {
            Ok(state) => {
                debug!("Transitioning: {} -> {}", self.state, state);
                
                self.state = state;
                Ok(match self.state {
                    SmtpState::QUIT => StateKind::QUIT,
                    _ => StateKind::CONTINUE
                })
            },
            Err(err) => match self.handle_smtp_error(err).await {
                Ok(_) => Ok(StateKind::QUIT),
                Err(io_err) => Err(io_err)
            }
        }
    }

    /**
    Sends an appropriate error response for an SMTP error and returns the error again.
    
    If the SMTP error originates from an io:Error or an IO error occurs during this response,
    returns the causing io::Error instead. 
    */
    async fn handle_smtp_error(&mut self, error: SmtpError) -> Result<SmtpError, io::Error> {
        debug!("Sending error response for '{:?}'", error.kind);
        match error.kind {
            ErrorKind::BADCOMMAND => {
                self.send("500 Unrecognized command\r\n".to_string()).await
                    .and(Ok(error))
                    .or_else(|err| Err(err.io_error.unwrap()))
            },
            ErrorKind::BADSEQUENCE => {
                debug!("Bad sequence: {:?}", self.state_history);
                self.send("503 Bad sequence\r\n".to_string()).await
                    .and(Ok(error))
                    .or_else(|err| Err(err.io_error.unwrap()))
            },
            ErrorKind::BADAUTHMECH => {
                self.send("535 5.7.8 Invalid authentication mechanism\r\n".to_string()).await
                    .and(Ok(error))
                    .or_else(|err| Err(err.io_error.unwrap()))
            },
            ErrorKind::BADCREDENTIALS => {
                self.send("535 5.7.8 Unauthorized\r\n".to_string()).await
                    .and(Ok(error))
                    .or_else(|err| Err(err.io_error.unwrap()))
            },
            ErrorKind::DELIVERYERROR(_) | ErrorKind::INVALIDMAILBOX => {
                self.send("550 Requested action not taken: mailbox unavailable\r\n".to_string()).await
                    .and(Ok(error))
                    .or_else(|err| Err(err.io_error.unwrap()))
            },
            ErrorKind::IOERROR => Err(error.io_error.unwrap()),
        }
    }

    /**
    Handles a command that only needs a static response.

    # Arguments
    `cmd` command the client sent
    `expected_state` list of origin states that allow this command
    `response` response msg
    */
    async fn handle_static_cmd(&mut self, cmd: &Command, expected_states: Vec<SmtpState>, response: &str) -> Result<(), SmtpError> {
        match expected_states.contains(&self.state) {
            true => {
                self.send(response.to_string()).await
            }
            false => {
                debug!("Expected one out of '{:?}', got {}", expected_states, self.state);
                Err(SmtpError::bad_sequence(cmd, self.state.clone()))
            }
        }
    }
    
    fn build_ehlo_response(config: &Config) -> String {
        format!(
            "250-{}\r\n\
            250 AUTH PLAIN LOGIN\r\n"
            , config.hostname
        ).to_string()
    }
    
    #[cfg(test)]
    pub(crate) fn ehlo_response(config: &Config) -> String {
        Self::build_ehlo_response(config)
    }

    /**
    Handles an EHLO command.
    Sends a list of available extensions.
    */
    async fn handle_ehlo(&mut self, cmd: Command) -> Result<(), SmtpError> {
        match self.state {
            SmtpState::INIT => {
                self.send(Self::build_ehlo_response(&self.config)).await
            },
            _ => {
                debug!("Bad sequence: Unexpected '{}' after '{}', expected to be in state INIT instead",
                    SmtpState::EHLO, self.state);
                Err(SmtpError::bad_sequence(&cmd, self.state.clone()))
            }
        }
    }

    /**
    Flushes the write buffer of `self.auth` in case SASL wants to write something.
    */
    async fn auth_flush(&mut self) -> Result<(), SmtpError> {
        self.auth.as_mut().expect("missing auth object")
            .flush(&mut self.conn_writer, "334 ".to_string(), "\r\n")
            .await
            .or_else(|e| Err(SmtpError::from_io(e)))
    }
    
    /**
    Handles an AUTH command. Starts the SASL auth process.
    */
    async fn handle_auth(&mut self, cmd: Command) -> Result<SmtpState, SmtpError> {
        // Catch incorrect state
        match self.state {
            SmtpState::EHLO => {},
            _ => return Err(SmtpError::bad_sequence(&cmd, self.state.clone()))
        };

        // Parse AUTH <mech> [mech_arg]
        let (mech, mech_arg) = match RE.auth_cmd.captures(cmd.message.as_str()) {
            Some(v) => (v.get(1), v.get(2)),
            None => (None, None)
        };
        
        let mech = match mech {
            Some(re_match) => String::from(re_match.as_str())
                .to_ascii_uppercase(),
            None => return Err(SmtpError::bad_auth_mech(&cmd, self.state.clone()))
        };
        
        let mech_arg = mech_arg
            .and_then(|re_match| Some(String::from(re_match.as_str())));
        
        self.auth = match auth::Auth::new(self.user_db.clone(), mech, mech_arg) {
            Ok(auth) => Some(auth),
            Err(auth::Error::InvalidMechanism) => return Err(SmtpError::bad_auth_mech(&cmd, self.state.clone())),
            Err(auth::Error::AuthUnsuccessful) => return Err(SmtpError::bad_credentials(&cmd, self.state.clone()))
        };
        self.auth_flush().await?;

        debug!("authorized: '{:?}'", self.auth.as_ref().unwrap().authorized());

        match self.auth.as_ref().unwrap().authorized() {
            Ok(Some(authorized)) => {
                self.finalize_authorization(authorized).await
            },
            Ok(None) => {
                Err(SmtpError::bad_credentials(&cmd, self.state.clone()))
            },

            // SASL not finished yet, keep going
            Err(_) => {
                Ok(SmtpState::AUTH)
            }
        }
    }

    /**
    Handles AUTH (SASL) steps once this protocol is in AUTH state.
    Returns to EHLO state once the SASL process is done (with or without successful auth).
    */
    async fn handle_auth_step(&mut self, cmd: &Command) -> Result<SmtpState, SmtpError> {
        match self.auth.as_mut().unwrap().step(Some(cmd.msg_strip_lf()?.as_ref())) {
            Ok(None) => {
                self.auth_flush().await?;
                Ok(SmtpState::AUTH)
            },
            Ok(Some(user)) => {
                self.finalize_authorization(user).await
            },
            Err(_) => {
                Err(SmtpError::bad_credentials(cmd, self.state.clone()))
            }
        }
    }

    /**
    Sends success message and handles Smtp state for an authorization success.
    */
    async fn finalize_authorization(&mut self, user: User) -> Result<SmtpState, SmtpError> {
        debug!("Finalizing auth");
        self.user = Some(user);
        self.send("235 2.7.0 Authentication successful\r\n".to_string())
            .await
            .and(Ok(SmtpState::EHLO))
    }

    /**
    Handles an incoming MAIL command.
    Stores the sender address that was sent and sends a 250 response.

    Verifies protocol state machine.
    Returns an SmtpError if protocol is violated or on IO error.
    */
    async fn receive_mail_cmd(&mut self, cmd: &Command) -> Result<(), SmtpError> {
        debug!("Handling MAIL: {}", cmd);
        match self.state {
            SmtpState::HELO | SmtpState::EHLO => {
                let sender = self.parse_address_message_by_re(
                    Regex::new(r"^MAIL FROM:<([^>]+)>\r\n$").unwrap(), cmd
                )?;
                self.mail.sender = Some(sender);
                self.send("250 OK\r\n".to_string()).await?;
                Ok(())
            },
            _ => {
                debug!("Bad sequence: Unexpected '{}' after '{}', expected to be in state HELO|EHLO instead",
                    SmtpState::MAIL, self.state);
                Err(SmtpError::bad_sequence(&cmd, self.state.clone()))
            }
        }
    }

    /**
    Handles an incoming RCPT command.
    Pushes the receiving address that was sent and sends a 354 response.
    
    Verifies protocol state machine.
    Returns an SmtpError if protocol is violated or on IO error.
    */
    async fn receive_rcpt(&mut self, cmd: &Command) -> Result<(), SmtpError> {
        debug!("Handling RCPT: {cmd}");
        match self.state {
            SmtpState::RCPT | SmtpState::MAIL => {
                let recipient = self.parse_address_message_by_re(
                    Regex::new(r"^RCPT TO:<([^>]+)>\r\n$").unwrap(), cmd
                )?;
                let recipient = MailAddress::new(recipient.as_str())
                    .map_err(|_| SmtpError::invalid_mailbox(cmd, self.state.clone()))?;
                
                self.mail.recipients.push(recipient);
                self.send("250 OK\r\n".to_string()).await?;
                Ok(())
            },
            _ => {
                Err(SmtpError::bad_sequence(&cmd, self.state.clone()))
            }
        }
    }
    
    /**
    Parses an address from an RCPT or MAIL command.
    Returns the contained address or `SmtpError::BADCOMMAND` on error.
    */
    fn parse_address_message_by_re(&mut self, re: Regex, cmd: &Command) -> Result<String, SmtpError> {
        let parse = match re.captures(&cmd.message) {
            Some(capture) => match capture.get(1) {
                Some(rcpt) => Some(rcpt.as_str().to_string()),
                None => None
            },
            None => None
        };
        
        match parse {
            Some(rcpt) => Ok(rcpt),
            None => Err(SmtpError::bad_command(&cmd).push_state(self.state.clone()))
        }
    }

    /**
    Attempts to deliver a mail to its recipients. Returns true only if the mail could be delivered
    to all recipients.
    */
    async fn deliver_mail(&self) -> Result<(), DeliveryError> {
        let mut mailboxes: Vec<(&MailAddress, String)> = Vec::new();
        for rcpt in &self.mail.recipients {
            let mb = match self.user_db.lock().unwrap().get_mailboxhome(rcpt) {
                Ok(mb) => mb,
                Err(_) => return Err(DeliveryError::NoSuchUser(rcpt.address.clone()))
            };
            mailboxes.push((rcpt, mb));
        }
        
        for (rcpt, mb) in mailboxes {
            match self.storage.store(&self.mail, rcpt, mb.clone()).await {
                Ok(()) => {},
                Err(err) => {
                    debug!("Error storing mail for '{:?}': '{}'", mb, err);
                    return Err(DeliveryError::MailboxIO(format!("{}", err)));
                }
            }
        }
        
        Ok(())
    }
    
    /**
    Handles incoming mail data input.
    Delivers the mail that was received and sends a 250 response.

    Verifies protocol state machine.
    Returns an SmtpError if protocol is violated or on IO error.
     */
    async fn receive_data(&mut self, cmd: Command) -> Result<SmtpState, SmtpError> {
        debug!("Handling data input: {cmd}");
        match self.state {
            SmtpState::DATA => {
                debug!("Mail!: {}", cmd.message);
                let mail_end = self.decode_transparency(cmd.message);
                match mail_end {
                    true => {
                        self.mail.finish();
                        match self.deliver_mail().await {
                            Ok(()) => {
                                self.send("250 OK\r\n".to_string()).await?;
                                Ok(SmtpState::DATAINPUT)
                            },
                            Err(err) => Err(err.into())
                        }
                    },
                    false => Ok(SmtpState::DATA)
                }
            }
            _ => {
                println!("Unexpected '{}' after '{}', expected mail input instead", SmtpState::DATAINPUT, self.state);
                Err(SmtpError::bad_sequence(&cmd, self.state.clone()))
            }
        }
    }

    /**
    Decodes mails as per transparency procedure in RFC5321#4.5.2 and pushes the result to Smtp.mail.
    */
    pub(crate) fn decode_transparency(&mut self, s: String) -> bool {
        debug!("Decoding transparency for \"{s}\"");
        let mut buf: Vec<&str> = Vec::new();
        let mut capacity = 0;
        let mut has_changed = false;
        
        // Catch empty mail; treat period on first line as end of mail
        match s.starts_with(".\r\n") {
            true => {
                self.mail.body = ".\r\n".to_string();
                return true;
            }
            false => {}
        }

        // Find and store \r\n.\r\n positions
        let (mail_end_start, mail_end_end, mail_is_complete) = match RE.mail_end.find(&s) {
            Some(m) => (m.start(), m.end(), true),
            None => (s.len(), s.len(), false)
        };


        // Handle transparency on first line
        let mut chunk_start = match s.starts_with(".") {
            true => {
                has_changed = true;
                1
            },
            false => 0
        };
        
        // Write every chunk between two \r\n. into buf to assemble the new body later
        let mut end_loop = false;
        while !end_loop {
            
            let chunk_end = match RE.period_linestart.find(&s[chunk_start..mail_end_start]) {
                Some(m) => {
                    has_changed = true;
                    m.start() + "\r\n".len() + 1
                },
                None => {
                    end_loop = true;
                    mail_end_end
                }
            };
            
            buf.push(&s[chunk_start..chunk_end]);
            capacity += chunk_end - chunk_start;
            debug!("Recognized mail part with len {}:\n{:?}", chunk_end - chunk_start, &s[chunk_start..chunk_end]);
            
            chunk_start = chunk_end + ".".len();
        }
        
        match has_changed {
            true => {
                self.mail.body.reserve(capacity);
                for el in buf {
                    self.mail.body += el;
                }
            },
            false => self.mail.body.push_str(s.as_str())
        };

        mail_is_complete
    }
}