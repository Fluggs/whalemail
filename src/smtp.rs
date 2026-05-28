use std::{fmt, io};
use strum::{Display, EnumString};
use strum_macros::IntoStaticStr;
use log::{debug};
use regex::Regex;
use crate::net::ConnectionHandler;
use crate::tests::SmtpTest;
use crate::smtp_error::{ErrorKind, SmtpError};
use crate::smtp_message::SmtpMessage;

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

pub struct Smtp {
    pub(crate) conn: Option<ConnectionHandler>,
    pub(crate) conn_testbed: Option<SmtpTest>,
    pub(crate) closed: bool,
    
    pub(crate) state: SmtpState,
    state_history: Vec<SmtpState>,
    pub(crate) mail: SmtpMessage,
    pub(crate) last_cmd_complete: bool,
    pub(crate) msg_buf: String,
}

impl Smtp {
    pub fn new (connhandler: ConnectionHandler) -> Smtp {
        Smtp {
            conn: Some(connhandler),
            conn_testbed: None,
            closed: false,
            state: SmtpState::INIT,
            state_history: Vec::new(),
            mail: SmtpMessage::new(),
            last_cmd_complete: true,
            msg_buf: "".to_string(),
        }
    }
    #[cfg(test)]
    pub fn new_testbed (testbed: SmtpTest) -> Smtp {
        Smtp {
            conn: None,
            conn_testbed: Some(testbed),
            closed: false,
            state: SmtpState::INIT,
            state_history: Vec::new(),
            mail: SmtpMessage::new(),
            last_cmd_complete: true,
            msg_buf: "".to_string(),
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

        let r = match &cmd.verb {
            Some(SmtpState::HELO) => {
                self.handle_simple_cmd(&cmd, vec![SmtpState::INIT], "250 OK\r\n")
                    .await
                    .and(Ok(SmtpState::HELO))
            },
            Some(SmtpState::MAIL) => {
                self.receive_mail_cmd(cmd).await
                    .and(Ok(SmtpState::MAIL))
            },
            Some(SmtpState::RCPT) => {
                self.receive_rcpt(cmd).await
                    .and(Ok(SmtpState::RCPT))
            },
            Some(SmtpState::DATA) => {
                self.handle_simple_cmd(&cmd, vec![SmtpState::RCPT], "354 start mail input\r\n")
                    .await
                    .and(Ok(SmtpState::DATA))
            },
            Some(SmtpState::DATAINPUT) => {
                self.receive_data(cmd).await.and(Ok(SmtpState::DATAINPUT))
            },
            Some(SmtpState::QUIT) => {
                self.handle_simple_cmd(&cmd, vec![SmtpState::DATAINPUT], "221 closing channel\r\n")
                    .await
                    .and(Ok(SmtpState::QUIT))
            },
            None => {
                debug!("Handling None");
                match &self.state {
                    SmtpState::DATA => {
                        self.handle_simple_cmd(&cmd, vec![SmtpState::DATA], "250 OK\r\n")
                            .await
                            .and(Ok(SmtpState::DATAINPUT))
                    },
                    _ => Err(SmtpError::bad_command(cmd))
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
    
    If the SMTP error originates from an IO error or an IO error occurs during this response,
    returns the io::Error instead. 
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
    async fn handle_simple_cmd(&mut self, cmd: &Command, expected_states: Vec<SmtpState>, response: &str) -> Result<(), SmtpError> {
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
                debug!("Bad sequence: Expected to be in state HELO|EHLO, got '{}'", self.state);
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
    async fn receive_data(&mut self, cmd: Command) -> Result<(), SmtpError> {
        debug!("Handling data input: {cmd}");
        match self.state {
            SmtpState::DATA => {
                debug!("Mail!: {}", cmd.message);
                // todo store msg
                self.send("250 OK\r\n".to_string()).await?;
                Ok(())
            }
            _ => {
                println!("Unexpected '{}' after '{}', expected mail input instead", SmtpState::DATAINPUT, self.state);
                Err(SmtpError::bad_sequence(&cmd, self.state.clone()))
            }
        }
    }
}