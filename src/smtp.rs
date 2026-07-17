use std::{fmt, io, sync};
use strum::{Display, EnumString};
use strum_macros::IntoStaticStr;
use log::{debug, info};
use regex::Regex;
use crate::net::ConnectionHandler;
use crate::tests::SmtpTest;
use crate::smtp_error::{ErrorKind, SmtpError};
use crate::smtp_mail::SmtpMail;
use crate::storage::Storage;

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
}

// Regex Patterns
struct Patterns {
    mail_end: Regex,
    period_linestart: Regex,
}

static RE: sync::LazyLock<Patterns> = sync::LazyLock::new(|| Patterns {
    mail_end: Regex::new(r"\r\n\.\r\n").unwrap(),
    period_linestart: Regex::new(r"\r\n\.").unwrap(),
});

pub struct Smtp {
    pub(crate) conn: Option<ConnectionHandler>,
    pub(crate) conn_testbed: Option<SmtpTest>,
    pub(crate) closed: bool,
    
    pub(crate) state: SmtpState,
    state_history: Vec<SmtpState>,
    pub(crate) mail: SmtpMail,
    
    storage: Storage,
}

impl Smtp {
    pub fn new (connhandler: ConnectionHandler, storage: Storage) -> Smtp {
        Smtp {
            conn: Some(connhandler),
            conn_testbed: None,
            closed: false,
            state: SmtpState::INIT,
            state_history: Vec::new(),
            mail: SmtpMail::new(),
            storage,
        }
    }
    #[cfg(test)]
    pub fn new_testbed (testbed: SmtpTest, storage: Storage) -> Smtp {
        Smtp {
            conn: None,
            conn_testbed: Some(testbed),
            closed: false,
            state: SmtpState::INIT,
            state_history: Vec::new(),
            mail: SmtpMail::new(),
            storage,
        }
    }
    
    async fn send(&mut self, s: String) -> Result<(), SmtpError> {
        match &self.conn {
            Some(c) => c.send(s).await
                .or_else(|error| Err(SmtpError::from_io(error, self.state.clone()))),
            None => {
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
                self.handle_static_cmd(&cmd, vec![SmtpState::INIT], "502 sorry\r\n")
                    .await
                    .and(Ok(SmtpState::INIT))
            },
            // HELO|EHLO -> MAIL
            Some(SmtpState::MAIL) => {
                self.receive_mail_cmd(cmd).await
                    .and(Ok(SmtpState::MAIL))
            },
            // MAIL|RCPT -> RCPT
            Some(SmtpState::RCPT) => {
                self.receive_rcpt(cmd).await
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
                    _ => {
                        info!("Unrecognized SMTP message: \"{}\"", cmd.message);
                        Err(SmtpError::bad_command(cmd))
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

    /**
    Handles an incoming MAIL command.
    Stores the sender address that was sent and sends a 250 response.

    Verifies protocol state machine.
    Returns an SmtpError if protocol is violated or on IO error.
    */
    async fn receive_mail_cmd(&mut self, cmd: Command) -> Result<(), SmtpError> {
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
    async fn receive_rcpt(&mut self, cmd: Command) -> Result<(), SmtpError> {
        debug!("Handling RCPT: {cmd}");
        match self.state {
            SmtpState::RCPT | SmtpState::MAIL => {
                let recipient = self.parse_address_message_by_re(
                    Regex::new(r"^RCPT TO:<([^>]+)>\r\n$").unwrap(), cmd
                )?;
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
    fn parse_address_message_by_re(&mut self, re: Regex, cmd: Command) -> Result<String, SmtpError> {
        let parse = match re.captures(&cmd.message) {
            Some(capture) => match capture.get(1) {
                Some(rcpt) => Some(rcpt.as_str().to_string()),
                None => None
            },
            None => None
        };
        
        match parse {
            Some(rcpt) => Ok(rcpt),
            None => Err(SmtpError::bad_command(cmd).push_state(self.state.clone()))
        }
    }
    
    /**
    Handles incoming mail data input.
    Stores the message that was sent and sends a 250 response.

    Verifies protocol state machine.
    Returns an SmtpError if protocol is violated or on IO error.
     */
    async fn receive_data(&mut self, cmd: Command) -> Result<SmtpState, SmtpError> {
        debug!("Handling data input: {cmd}");
        match self.state {
            SmtpState::DATA => {
                debug!("Mail!: {}", cmd.message);
                let mail_end = self.decode_transparency(cmd.message);
                self.send("250 OK\r\n".to_string()).await?;
                match mail_end {
                    true => {
                        self.mail.finish();
                        self.storage.store(&self.mail).await.unwrap(); //todo error handling
                        Ok(SmtpState::DATAINPUT)
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