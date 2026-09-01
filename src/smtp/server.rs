use std::{fmt, io, sync};
use std::fmt::Debug;
use strum::{Display, EnumString};
use strum_macros::{IntoStaticStr};
use log::{debug};
use regex::Regex;
use crate::config::Config;
use crate::auth::auth;
use crate::userdb::userdb::UserDBMtx;
use crate::net::{ConnectionHandler, IO};
use crate::tests::test::SmtpTest;
use crate::smtp::error::DeliveryError;
use crate::smtp::envelope::{Envelope, MailAddress};
use crate::maildir::Storage;
use crate::user::User;

static MSG_INVALID_MAILBOX: &str = "450 Invalid mailbox\r\n";
static MSG_INVALID_HOST: &str = "450 Invalid host\r\n";
static MSG_MAILBOX_UNAVAILABLE: &str = "450 Requested mail action not taken: mailbox unavailable\r\n";
static MSG_BAD_COMMAND: &str = "500 Unrecognized command\r\n";
static MSG_UNAUTHORIZED: &str = "530 5.7.0 Authentication required\r\n";

#[derive(Debug, Clone, PartialEq, Display, EnumString, IntoStaticStr)]
pub(crate) enum CommandVerb {
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
    pub(crate) verb: Option<CommandVerb>,
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
            Some(first) => first.parse::<CommandVerb>().ok(),
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
    fn msg_strip_lf(&self) -> String {
        let r = match self.message.ends_with("\r\n") {
            true => self.message[0..self.message.len() - 2].to_string(),
            false => self.message.to_string()
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
    pub(crate) async fn send(&mut self, s: String) -> Result<(), io::Error> {
        match &mut self.conn {
            Some(c) => Ok(c.send(s).await?),
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
struct BadCommandError {}

impl BadCommandError {
    async fn respond<T: IO>(&self, writer: &mut ConnectionWriter<T>) -> Result<(), io::Error> {
        Self::write_msg(writer).await
    }
    
    async fn write_msg<T: IO>(writer: &mut ConnectionWriter<T>) -> Result<(), io::Error> {
        writer.send(MSG_BAD_COMMAND.to_string()).await
    }
}

#[derive(Debug)]
struct InitState {
}

impl InitState {
    pub(crate) async fn greet<T: IO>(writer: &mut ConnectionWriter<T>, _config: &Config) -> Result<Self, io::Error> {
        // todo refine greeting message
        writer.send("220 hi\r\n".to_string()).await?;
        Ok(Self {})
    }
}

#[derive(Debug)]
struct HeloState {
}

impl HeloState {
    async fn respond<T: IO>(writer: &mut ConnectionWriter<T>, _from: InitState) -> Result<Self, io::Error> {
        writer.send("250 OK\r\n".to_string()).await?;
        Ok(Self {})
    }
}

#[derive(Debug)]
struct EhloState {
    authorized: Option<User>
}

impl EhloState {
    fn build_ehlo_response(config: &Config) -> String {
        format!(
            "250-{}\r\n\
            250 AUTH PLAIN LOGIN\r\n"
            , config.hostname
        ).to_string()
    }
    async fn respond<T: IO>(writer: &mut ConnectionWriter<T>, config: &Config, _from: InitState) -> Result<Self, io::Error> {
        writer.send(EhloState::build_ehlo_response(config)).await?;
        Ok(Self { authorized: None })
    }
    
    fn push_authorized(&mut self, user: User) {
        self.authorized = Some(user)
    }
}

#[cfg(test)]
pub(crate) fn ehlo_response(config: &Config) -> String {
    EhloState::build_ehlo_response(config)
}

enum AuthResult {
    Unfinished(AuthState),
    Authorized((EhloState, User)),
    BadMechanism(EhloState),
    BadCredentials(EhloState),
}

impl AuthResult {
    async fn respond<T: IO>(&self, writer: &mut ConnectionWriter<T>) -> Result<(), io::Error> {
        match self {
            AuthResult::Unfinished(_) => Ok(()),
            AuthResult::Authorized(_) => writer.send(
                "235 2.7.0 Authentication successful\r\n".to_string()).await,
            AuthResult::BadMechanism(_) => writer.send(
                "535 5.7.8 Invalid authentication mechanism\r\n".to_string()).await,
            AuthResult::BadCredentials(_) => writer.send(
                "535 5.7.8 Unauthorized\r\n".to_string()).await,
        }
    }
}

struct AuthState {
    auth: auth::Auth,
    from_state: EhloState,
}

impl AuthState {
    /**
    Handles an AUTH command by starting the SASL auth process.
    
    Returns:
    * `Ok(SmptState::AUTH(self))` on successful SASL initiation
    * `OK(SmtpState::EHLO(from))` on 
    */
    async fn init_sasl<T: IO>(
        writer: &mut ConnectionWriter<T>,
        cmd: Command,
        user_db: UserDBMtx,
        hostname: String,
        from: EhloState
    ) -> Result<AuthResult, io::Error> {
        // Parse AUTH <mech> [mech_arg]
        let (mech, mech_arg) = match RE.auth_cmd.captures(cmd.message.as_str()) {
            Some(v) => (v.get(1), v.get(2)),
            None => (None, None)
        };

        let mech = match mech {
            Some(re_match) => String::from(re_match.as_str())
                .to_ascii_uppercase(),
            None => return Ok(AuthResult::BadMechanism(from))
        };

        let mech_arg = mech_arg
            .and_then(|re_match| Some(String::from(re_match.as_str())));

        let auth = match auth::Auth::new(user_db.clone(), hostname, mech, mech_arg) {
            Ok(auth) => auth,
            Err(auth::Error::InvalidMechanism) => return Ok(AuthResult::BadMechanism(from)),
            Err(auth::Error::AuthUnsuccessful) => return Ok(AuthResult::BadCredentials(from))
        };
        
        let mut auth_state = Self {
            auth,
            from_state: from
        };
        
        auth_state.auth_flush(writer).await?;
        debug!("authorized: '{:?}'", auth_state.auth.authorized());

        // Handle whether the SASL session is already finished (e.g. PLAIN)
        match auth_state.auth.authorized() {
            Ok(Some(authorized)) => {
                Ok(AuthResult::Authorized((auth_state.from_state, authorized)))
            },
            Ok(None) => {
                Ok(AuthResult::BadCredentials(auth_state.from_state))
            },

            // SASL not finished yet, keep going
            Err(_) => { Ok(AuthResult::Unfinished(auth_state)) }
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
    ) -> Result<AuthResult, io::Error> {
        match self.auth.step(Some(cmd.msg_strip_lf().as_ref())) {
            Ok(None) => {
                self.auth_flush(writer).await?;
                Ok(AuthResult::Unfinished(self))
            },
            Ok(Some(user)) => {
                Ok(AuthResult::Authorized((self.from_state, user)))
            },
            Err(_) => {
                Ok(AuthResult::BadCredentials(self.from_state))
            }
        }
    }

    /**
    Flushes the write buffer of `self.auth` in case SASL wants to write something.
    */
    async fn auth_flush<T: IO>(&mut self, cw: &mut ConnectionWriter<T>) -> Result<(), io::Error> {
        self.auth
            .flush(cw, "334 ".to_string(), "\r\n")
            .await
    }
}

impl Debug for AuthState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "AuthState")
    }
}

/**
Parses an address from an RCPT or MAIL command.
Returns the contained address or `BadCommandError` on error.
*/
fn parse_address_message_by_re(re: Regex, cmd: &Command) -> Result<String, BadCommandError> {
    let parse = match re.captures(&cmd.message) {
        Some(capture) => match capture.get(1) {
            Some(rcpt) => Some(rcpt.as_str().to_string()),
            None => None
        },
        None => None
    };

    match parse {
        Some(rcpt) => Ok(rcpt),
        None => Err(BadCommandError {})
    }
}

struct MailFromState {
    sender: MailAddress,
    is_local_sender: bool,
    authorized: Option<User>,
}

impl MailFromState {
    async fn mail_from<T: IO>(
        writer: &mut ConnectionWriter<T>,
        user_db: UserDBMtx,
        cmd: Command,
        from_state: SmtpState
    ) -> Result<SmtpState, io::Error> {
        debug!("Handling MAIL: {}", cmd);
        
        let sender = match parse_address_message_by_re(
            Regex::new(r"^MAIL FROM:<([^>]+)>\r\n$").unwrap(), &cmd
        ) {
            Ok(s) => s,
            Err(bad_cmd) => {
                bad_cmd.respond(writer).await?;
                return Ok(from_state);
            }
        };
        
        let mut sender = match MailAddress::new(sender.as_str()) {
            Ok(sender) => sender,
            Err(_) => {
                writer.send(MSG_INVALID_HOST.to_string()).await?;
                return Ok(from_state);
            }
        };
        
        // Handle auth and permission
        let mut authorized = None;
        if let SmtpState::EHLO(ehlo) = from_state {
            if !Self::has_permission(user_db.clone(), &ehlo.authorized, &mut sender) {
                writer.send(MSG_UNAUTHORIZED.to_string()).await?;
                return Ok(SmtpState::EHLO(ehlo));
            }
            authorized = ehlo.authorized;
        }
        
        writer.send("250 OK\r\n".to_string()).await?;
        let is_local_sender = sender.cached_is_local(user_db);
        Ok(SmtpState::MAIL(Self {
            sender,
            is_local_sender,
            authorized,
        }))
    }
    
    /**
    Returns whether the sender is authorized to use the stated sender address.
    This is only false when the sender address is a local mailbox and `authorized` does not match the sender.
    */
    fn has_permission(user_db: UserDBMtx, authorized: &Option<User>, sender: &mut MailAddress) -> bool {
        let r = match sender.cached_is_local(user_db) {
            true => match authorized {
                None => false,
                Some(user) => {
                    debug!("user: '{}' @ '{}', sender: '{}' @ '{}'", user.identity, user.hostname, sender.local_part, sender.domain);
                    user.identity.eq(&sender.local_part) && user.hostname.eq(&sender.domain)
                }
            },
            false => true
        };
        
        debug!("'{:?}' has permission to send as '{}': '{}'", authorized.as_ref().map(|user| &user.identity), sender, r);
        r
    }
}

enum RcptError {
    BadCommand,
    InvalidMailbox,
    Unauthorized,
}

impl RcptError {
    async fn respond<T: IO>(&self, writer: &mut ConnectionWriter<T>) -> Result<(), io::Error> {
        match self {
            RcptError::BadCommand => BadCommandError::write_msg(writer).await,
            RcptError::InvalidMailbox => writer.send(MSG_INVALID_MAILBOX.to_string()).await,
            RcptError::Unauthorized => writer.send(MSG_UNAUTHORIZED.to_string()).await,
        }
    }
}

struct RcptState {
    sender: MailAddress,
    is_local_sender: bool,
    recipients: Vec<MailAddress>,
    authorized: Option<User>,
}

impl RcptState {
    /**
    Constructs an RcptState and handles adding the first recipient from an RCPT command.
    */
    async fn new<T: IO>(
        writer: &mut ConnectionWriter<T>,
        hostname: &str,
        cmd: Command,
        mut from_state: MailFromState
    ) -> Result<Self, Result<MailFromState, io::Error>> {
        let mut r = Self {
            sender: from_state.sender.clone(),
            is_local_sender: from_state.is_local_sender,
            recipients: Vec::new(),
            authorized: from_state.authorized.take(),
        };
        
        match r.add_rcpt(writer, hostname, cmd).await {
            Ok(_) => {
                Ok(r)
            },
            Err(Ok(rcpt_err)) => {
                match rcpt_err.respond(writer).await {
                    Ok(()) => {
                        from_state.authorized = r.authorized.take();
                        Err(Ok(from_state))
                    },
                    Err(io_err) => Err(Err(io_err))
                }
            },
            Err(Err(io_err)) => Err(Err(io_err))
        }
    }
    
    /**
    Determines whether the client has permission to send to a recipient address.
    */
    fn has_permission(hostname: &str, authorized: &Option<User>, rcpt: &mut MailAddress) -> bool {
        let r = match rcpt.domain.eq(hostname) {
            false => match authorized {
                None => false,
                Some(_) => {
                    true
                }
            },
            true => true
        };

        debug!("'{:?}' has permission to send to '{}': '{}'", authorized.as_ref().map(|user| &user.identity), rcpt, r);
        r
    }
    
    /**
    Parses an RCPT command and adds the resulting recipient.
    */
    async fn add_rcpt<T: IO>(&mut self, writer: &mut ConnectionWriter<T>, hostname: &str, cmd: Command)
        -> Result<(), Result<RcptError, io::Error>> {
        let recipient = parse_address_message_by_re(
            Regex::new(r"^RCPT TO:<([^>]+)>\r\n$").unwrap(), &cmd
        )
            .or_else(|_| Err(Ok(RcptError::BadCommand)))?;
        let mut recipient = MailAddress::new(recipient.as_str())
            .map_err(|_| RcptError::InvalidMailbox)
            .or_else(|smtp_err| Err(Ok(smtp_err)))?;
        
        if !Self::has_permission(hostname, &self.authorized, &mut recipient) {
            return Err(Ok(RcptError::Unauthorized));
        }
        
        self.recipients.push(recipient);
        writer.send("250 OK\r\n".to_string()).await
            .or_else(|ioerr| Err(Err(ioerr)))?;
        Ok(())
        
    }

    /**
    State rollback in case DATA command fails
    */
    fn from(envelope: Envelope, authorized: Option<User>, is_local_sender: bool) -> RcptState {
        RcptState {
            sender: envelope.sender,
            is_local_sender,
            recipients: envelope.recipients,
            authorized,
        }
    }
}

struct DataState {
    user_db: UserDBMtx,
    sender: MailAddress,
    is_local_sender: bool,
    recipients: Vec<MailAddress>,
    authorized: Option<User>,
    mail_body: String,
    body_finished: bool,
}

impl DataState {
    async fn new<T: IO>(
        writer: &mut ConnectionWriter<T>,
        user_db: UserDBMtx,
        from_state: RcptState)
        -> Result<Self, io::Error> {
        writer.send("354 start mail input\r\n".to_string()).await?;
        
        Ok(Self {
            user_db,
            sender: from_state.sender,
            is_local_sender: from_state.is_local_sender,
            recipients: from_state.recipients,
            authorized: from_state.authorized,
            mail_body: String::new(),
            body_finished: false,
        })
    }
    
    #[cfg(test)]
    fn mock(user_db: UserDBMtx) -> Self {
        Self {
            user_db,
            sender: MailAddress::mock(),
            is_local_sender: false,
            recipients: Vec::new(),
            authorized: None,
            mail_body: String::new(),
            body_finished: false,
        }   
    }
    
    async fn delivery_error_response<T: IO>(writer: &mut ConnectionWriter<T>) -> Result<(), io::Error> {
        writer.send(MSG_MAILBOX_UNAVAILABLE.to_string()).await
    }
    
    /**
    Handles incoming data after a DATA command
    */
    async fn receive_data<T: IO>(
        mut self,
        writer: &mut ConnectionWriter<T>,
        storage: &mut Storage,
        cmd: Command) 
    -> Result<SmtpState, io::Error> {
        debug!("Mail!: {}", cmd.message);
        let mail_end = self.decode_transparency(cmd.message);
        match mail_end {
            true => {
                self.body_finished = true;
                let mail = Envelope::new(self.sender, self.recipients, self.mail_body);
                match Self::deliver_mail(storage, self.user_db.clone(), &mail).await {
                    Ok(()) => {
                        writer.send("250 OK\r\n".to_string()).await?;
                        Ok(SmtpState::DATACOMPLETE(CompleteState::new(mail)))
                    },
                    Err(err) => {
                        debug!("Mail delivery error: {:?}", err);
                        Self::delivery_error_response(writer).await?;
                        Ok(SmtpState::RCPT(RcptState::from(mail, self.authorized, self.is_local_sender)))
                    }
                }
            },
            false => Ok(SmtpState::DATA(self))
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
                Err(err) => {
                    debug!("Mail delivery error: {:?}", err);
                    return Err(DeliveryError::NoSuchUser(rcpt.address.clone()))
                }
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
enum SmtpState {
    INIT(InitState),
    HELO(HeloState),
    EHLO(EhloState),
    AUTH(AuthState),
    MAIL(MailFromState),
    RCPT(RcptState),
    DATA(DataState),
    DATACOMPLETE(CompleteState),
    QUIT(CompleteState),
    CANCELLED,
}

impl Debug for SmtpState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s: &str = self.into();
        write!(f, "{}", s)
    }
}

pub(crate) struct SmtpServer<T: IO> {
    conn_writer: ConnectionWriter<T>,
    config: Config,
    state: SmtpState,
    state_history: Vec<SmtpState>,
    user_db: UserDBMtx,
    storage: Storage,
    authorized: Option<User>,
}

impl<T: IO> SmtpServer<T> {
    async fn build(
        mut cw: ConnectionWriter<T>,
        config: Config,
        user_db: UserDBMtx,
        storage: Storage
    ) -> Result<Self, io::Error> {
        let state = InitState::greet(&mut cw, &config).await?;

        Ok(Self {
            conn_writer: cw,
            config,
            state: SmtpState::INIT(state),
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
    ) -> Result<Self, io::Error> {
        let cw = ConnectionWriter {
            conn: Some(connhandler),
            conn_testbed: None,
        };
        
        Self::build(cw, config, user_db, storage).await
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
            
            (SmtpState::INIT(old_state), Some(CommandVerb::HELO)) => {
                HeloState::respond(&mut self.conn_writer, old_state)
                    .await
                    .and_then(|helo| Ok(SmtpState::HELO(helo)))?
            },
            
            (SmtpState::INIT(old_state), Some(CommandVerb::EHLO)) => {
                EhloState::respond(&mut self.conn_writer, &self.config, old_state)
                    .await
                    .and_then(|ehlo| Ok(SmtpState::EHLO(ehlo)))?
            },
            
            (SmtpState::EHLO(ehlo), Some(CommandVerb::AUTH)) => {
                let auth = AuthState::init_sasl(
                    &mut self.conn_writer,
                    cmd,
                    self.user_db.clone(),
                    self.config.hostname.clone(),
                    ehlo
                ).await?;
                auth.respond(&mut self.conn_writer).await?;
                Self::transition_auth_result(auth)?
            },
            
            (SmtpState::AUTH(auth), _) => {
                let auth = auth.handle_auth_step(&mut self.conn_writer, cmd).await?;
                auth.respond(&mut self.conn_writer).await?;
                Self::transition_auth_result(auth)?
            }
            
            (SmtpState::HELO(helo), Some(CommandVerb::MAIL)) => {
                MailFromState::mail_from(&mut self.conn_writer, self.user_db.clone(), cmd, SmtpState::HELO(helo))
                    .await?
            },
            
            (SmtpState::EHLO(ehlo), Some(CommandVerb::MAIL)) => {
                MailFromState::mail_from(&mut self.conn_writer, self.user_db.clone(), cmd, SmtpState::EHLO(ehlo))
                    .await?
            },
            
            (SmtpState::MAIL(mail), Some(CommandVerb::RCPT)) => {
                RcptState::new(&mut self.conn_writer, self.config.hostname.as_str(), cmd, mail)
                    .await
                    .and_then(|rcpt| Ok(SmtpState::RCPT(rcpt)))
                    .or_else(|res_mailfrom| Ok::<SmtpState, io::Error>(SmtpState::MAIL(res_mailfrom?)))?
            },
            
            (SmtpState::RCPT(mut rcpt), Some(CommandVerb::RCPT)) => {
                match rcpt.add_rcpt(&mut self.conn_writer, self.config.hostname.as_str(), cmd).await {
                    Ok(()) => Ok(SmtpState::RCPT(rcpt)),
                    Err(Ok(rcpt_err)) => {
                        rcpt_err.respond(&mut self.conn_writer).await?;
                        Ok(SmtpState::RCPT(rcpt))
                    },
                    Err(Err(io_err)) => Err(io_err)
                }?
            },
            
            (SmtpState::RCPT(rcpt), Some(CommandVerb::DATA)) => {
                DataState::new(&mut self.conn_writer, self.user_db.clone(), rcpt)
                    .await
                    .and_then(|state| Ok(SmtpState::DATA(state)))?
            },
            
            (SmtpState::DATA(state), _) => {
                state.receive_data(&mut self.conn_writer, &mut self.storage, cmd).await?
            },
            
            (SmtpState::DATACOMPLETE(state), Some(CommandVerb::QUIT)) => {
                state.quit(&mut self.conn_writer).await;
                SmtpState::QUIT(state)
            },
            
            (_state, Some(_verb)) => {
                debug!("Bad sequence: {:?}", self.state_history);
                self.conn_writer.send("503 Bad sequence\r\n".to_string()).await
                    .and(Ok(SmtpState::CANCELLED))
                    .or_else(|io_err| Err(io_err))?
            },

            (_state, None) => {
                debug!("Unrecognized command");
                self.conn_writer.send(MSG_BAD_COMMAND.to_string()).await
                    .and(Ok(SmtpState::CANCELLED))
                    .or_else(|io_err| Err(io_err))?
            }
        };
        
        match self.state {
            SmtpState::QUIT(_) | SmtpState::CANCELLED => Ok((self, StateKind::QUIT)),
            _ => Ok((self, StateKind::CONTINUE))
        }
    }
    
    /**
    Converts an AuthResult into the SmtpState that it results in.
    On auth success, fills the returned Option<User> with the authenticated user.
    */
    fn transition_auth_result(auth_result: AuthResult) -> Result<SmtpState, io::Error> {
        let r = match auth_result {
            AuthResult::Unfinished(auth) => SmtpState::AUTH(auth),
            AuthResult::Authorized((mut ehlo, user)) => {
                ehlo.push_authorized(user);
                SmtpState::EHLO(ehlo)
            },
            AuthResult::BadMechanism(ehlo) => SmtpState::EHLO(ehlo),
            AuthResult::BadCredentials(ehlo) => SmtpState::EHLO(ehlo)
        };
        
        debug!("Transitioning auth to '{:?}'", r);
        Ok(r)
    }

    #[cfg(test)]
    pub(crate) async fn new_testbed (
        testbed: SmtpTest,
        config: Config,
        user_db: UserDBMtx,
        storage: Storage
    ) -> Result<Self, io::Error> {
        let cw = ConnectionWriter {
            conn: None,
            conn_testbed: Some(testbed),
        };

        Self::build(cw, config, user_db, storage).await
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
            SmtpState::DATACOMPLETE(complete)
            | SmtpState::QUIT(complete) => {
                complete.mail()
            },
            _ => panic!("Incorrect smtp state: {:?}", self.state)
        }
    }

    #[cfg(test)]
        pub(crate) fn is_local_sender(&self) -> bool {
        match &self.state {
            SmtpState::RCPT(rcpt) => rcpt.is_local_sender,
            SmtpState::MAIL(mail) => mail.is_local_sender,
            _ => panic!("Incorrect smtp state: {:?}", self.state)
        }
    }

    #[cfg(test)]
    pub(crate) fn recipients(&self) -> &Vec<MailAddress> {
        match &self.state {
            SmtpState::RCPT(rcptstate) => &rcptstate.recipients,
            SmtpState::DATA(datastate) => &datastate.recipients,
            SmtpState::DATACOMPLETE(complete)
            | SmtpState::QUIT(complete) => {
                &complete.mail().recipients
            },
            _ => panic!("Incorrect smtp state: {:?}", self.state)
        }
    }

    #[cfg(test)]
    pub(crate) fn config(&self) -> &Config {
        &self.config
    }
    
    #[cfg(test)]
    pub(crate) fn decode_transparency(&self, s: String) -> (bool, String) {
        let mut data = DataState::mock(self.user_db.clone());
        let r = data.decode_transparency(s);
        (r, data.mail_body)
    }
}
