use std::{fmt, io};
use strum::{Display, EnumString};
use strum_macros::IntoStaticStr;
use log::{warn, debug};
use crate::net::ConnectionHandler;
use crate::util::string_as_bytes;
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
    CANCELLED,
    IOERROR,
}

impl SmtpState {
    /// Returns the smtp state that corresponds to an smtp error.
    fn from_error_kind(errorkind: ErrorKind) -> Self {
        match errorkind {
            ErrorKind::BADCOMMAND => { Self::CANCELLED }
            ErrorKind::BADSEQUENCE => { Self::CANCELLED }
            ErrorKind::IOERROR => { Self::IOERROR }
        }
    }
}

struct StateTransition {
    next_state: SmtpState,
    state_kind: StateKind,
}

impl From<SimpleResponse> for StateTransition {
    fn from(res: SimpleResponse) -> Self {
        StateTransition {
            next_state: res.expect,
            state_kind: res.next_state_kind,
        }
    }
}

impl From<SmtpState> for StateTransition {
    fn from(state: SmtpState) -> Self {
        StateTransition {
            next_state: state,
            state_kind: StateKind::CONTINUE,
        }
    }
}

#[derive(Clone, Debug)]
#[derive(PartialEq)]
pub enum StateKind {
    CONTINUE,
    QUIT
}

pub(crate) struct Command {
    pub(crate) verb: Option<SmtpState>,
    pub(crate) remainder: String,
}

/// Used for the simple parts of the protocol. If the Command type `expected` comes in, respond
/// with `response` and move to state `next_state`.
struct SimpleResponse {
    expect: SmtpState,
    response: String,
    next_state_kind: StateKind,
}

impl fmt::Display for Command {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match &self.verb {
            Some(verb) => write!(f, "{} {}", verb, self.remainder),
            None => write!(f, "{}", self.remainder),
        }
        
    }
}

impl Command {
    fn new(s: String) -> Result<Command, SmtpError> {
        let mut split = s.trim().split(" ");
        let verb: &str = match split.next() {
            Some(r) => r,
            None => {
                return Err(SmtpError {
                    kind: ErrorKind::BADCOMMAND,
                    state: None,
                    cmd: s,
                    io_error: None,
                });
            }
        }
            .trim();
        
        let verb = match verb.parse::<SmtpState>() {
            Ok(r) => Some(r),
            Err(_) => None,
        };

        let r = Command {
            verb,
            remainder: split.collect::<Vec<_>>().join(" "),
        };

        Ok(r)
    }
}

pub struct Smtp {
    pub conn: Option<ConnectionHandler>,
    pub conn_testbed: Option<SmtpTest>,
    pub closed: bool,
    
    pub(crate) state: SmtpState,
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
                self.conn_testbed.as_mut().unwrap().send(s)
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
        let cmd = match Command::new(input) {
            Ok(r) => r,
            Err(err) => {
                warn!("Unexpected SMTP command message: '{}' ({})", err.cmd, string_as_bytes(&err.cmd));
                return Ok(StateKind::QUIT)
            }
        };

        debug!("Command: {}", match &cmd.verb {
            Some(v) => format!("{v}"),
            None => "<data input>".to_string()
        });

        /*let r = match cmd.verb {
            Some(SmtpState::HELO) => {

            }
        }*/

        let r = match self.state {

            // INIT -> HELO, INIT -> EHLO, FAIL -> INIT
            // todo timeout; todo avoid infinite loop INIT -> FAIL -> INIT
            SmtpState::INIT => self.expect_simple_command(cmd, Box::from([
                SimpleResponse {
                    expect: SmtpState::HELO,
                    response: "250 OK\r\n".to_string(),
                    next_state_kind: StateKind::CONTINUE,
                },
                SimpleResponse {
                    expect: SmtpState::EHLO,
                    response: "250 OK\r\n".to_string(),
                    next_state_kind: StateKind::CONTINUE,
                },
            ]),
            SimpleResponse {
                    expect: SmtpState::INIT,
                    response: "554 what u doing\r\n".to_string(),
                    next_state_kind: StateKind::CONTINUE
                }
            ).await,

            // EHLO -> MAIL
            SmtpState::EHLO => self.expect_simple_command(
                cmd, Box::from([
                    SimpleResponse {
                        expect: SmtpState::MAIL,
                        response: "250 OK\r\n".to_string(),
                        next_state_kind: StateKind::CONTINUE
                    }
                ]),
                SimpleResponse {
                    expect: SmtpState::INIT,
                    response: "554 what u doing\r\n".to_string(),
                    next_state_kind: StateKind::CONTINUE
                }
            ).await,

            // HELO -> MAIL
            SmtpState::HELO => self.expect_simple_command(
                cmd, Box::from([
                    SimpleResponse {
                        expect: SmtpState::MAIL,
                        response: "250 OK\r\n".to_string(),
                        next_state_kind: StateKind::CONTINUE
                    }
                ]),
                SimpleResponse {
                    expect: SmtpState::INIT,
                    response: "554 what u doing\r\n".to_string(),
                    next_state_kind: StateKind::CONTINUE
                }
            ).await,

            // MAIL -> RCPT
            SmtpState::MAIL => self.expect_simple_command(
                cmd, Box::from([
                    SimpleResponse {
                        expect: SmtpState::RCPT,
                        response: "250 OK\r\n".to_string(),
                        next_state_kind: StateKind::CONTINUE
                    }
                ]),
                SimpleResponse {
                    expect: SmtpState::MAIL,
                    response: "554 what u doing\r\n".to_string(),
                    next_state_kind: StateKind::CONTINUE
                }
            ).await,

            // DATAINPUT -> QUIT
            SmtpState::DATAINPUT => self.expect_simple_command(
                cmd, Box::from([
                    SimpleResponse {
                        expect: SmtpState::QUIT,
                        response: "221 closing channel\r\n".to_string(),
                        next_state_kind: StateKind::QUIT
                    }
                ]),
                SimpleResponse {
                    expect: SmtpState::QUIT,
                    response: "554 what u doing\r\n".to_string(),
                    next_state_kind: StateKind::QUIT
                }
            ).await,
            SmtpState::RCPT => self.state_rcpt(cmd).await,
            SmtpState::DATA => self.state_data(cmd).await,
            SmtpState::CANCELLED => self.state_cancelled(cmd).await,
            SmtpState::IOERROR => self.state_ioerror(cmd).await,
            SmtpState::QUIT => self.state_cancelled(cmd).await,
        };

        match r {
            Ok(transition) => {
                debug!("Transitioning: {} -> {} ({:?})", self.state, transition.next_state, transition.state_kind);
                self.state = transition.next_state;
                Ok(transition.state_kind)
            },

            // Pass io errors up or handle protocol errors
            Err(err) => {
                let r = match self.handle_smtp_error(err).await {
                    Ok(err) => {
                        self.state = SmtpState::from_error_kind(err.kind);
                        Ok(StateKind::QUIT)
                    }
                    Err(e) => {
                        self.state = SmtpState::IOERROR;
                        Err(e)
                    }
                };
                r
            }
        }
    }

    /**
    Sends an appropriate error message
    */
    async fn handle_smtp_error(&mut self, error: SmtpError) -> Result<SmtpError, io::Error> {
        match error.kind {
            ErrorKind::BADCOMMAND => {
                self.send("500 Unrecognized command".to_string()).await
                    .and(Ok(error))
                    .or_else(|err| Err(err.io_error.unwrap()))
            },
            ErrorKind::BADSEQUENCE => {
                self.send("503 Bad sequence".to_string()).await
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
    async fn handle_simple_cmd(&mut self, cmd: Command, expected_states: Vec<SmtpState>, response: &str) -> Result<(), SmtpError> {
        match expected_states.contains(&self.state) {
            true => {}
            false => {
                return Err(SmtpError::bad_command(cmd))
            }
        }
        
        //self.send(response.to_string()).await.into()?;

        Ok(())
    }

    async fn expect_simple_command(&mut self, cmd: Command, paths: Box<[SimpleResponse]>,
                                   fail_path: SimpleResponse) -> Result<StateTransition, SmtpError> {
        for path in paths {
            match cmd.verb {
                Some(verb) if verb == path.expect => {
                    self.send(path.response.clone()).await?;
                    return Ok(StateTransition::from(path));
                }
                Some(_) => (),
                None => break
            }
        }
        
        self.send(fail_path.response.clone()).await?;
        Ok(StateTransition::from(fail_path))
    }

    async fn state_rcpt(&mut self, cmd: Command) -> Result<StateTransition, SmtpError> {
        debug!("{cmd}");
        match cmd.verb {
            Some(SmtpState::RCPT) => {
                match self.push_rcpt(cmd) {
                    Ok(()) => { Ok(StateTransition::from(SmtpState::RCPT)) },
                    Err(err) => { Err(err) }
                }
            },
            Some(SmtpState::DATA) => {
                self.send("354 start mail input\r\n".to_string()).await?;
                Ok(StateTransition::from(SmtpState::DATA))
            },
            _ => {
                self.send("554 leave me alone\r\n".to_string()).await?;
                Ok(StateTransition::from(SmtpState::CANCELLED))
            }
        }
    }
    
    fn push_rcpt(&mut self, cmd: Command) -> Result<(), SmtpError> {
        let mut split = cmd.remainder.trim().splitn(1, " ");
        match split.next() {
            Some("TO:") => {},
            _ => {
                return Err(SmtpError::bad_command(cmd)
                    .push_state(self.state.clone())
                );
            }
        };
        
        match split.next() {
            Some(s) => {
                // Empty recipient
                if s.trim().len() == 0 {
                    return Err(SmtpError::bad_command(cmd)
                        .push_state(self.state.clone())
                    );
                };
                self.mail.recipients.push(s.to_string());
                Ok(())
            },
            None => Err(SmtpError::bad_command(cmd)
                .push_state(self.state.clone())
            )
        }
    }

    async fn state_data(&mut self, cmd: Command) -> Result<StateTransition, SmtpError> {
        debug!("{cmd}");
        match cmd.verb {
            None => {
                debug!("Mail!: {}", cmd.remainder);
                self.send("250 OK\r\n".to_string()).await?;
                Ok(StateTransition::from(SmtpState::DATAINPUT))
            }
            Some(v) => {
                println!("Unexpected '{}' after '{}', expected mail input instead", v, self.state);
                self.send("554 leave me alone\r\n".to_string()).await?;
                Ok(StateTransition::from(SmtpState::CANCELLED))
            }
        }
    }

    async fn state_cancelled(&mut self, cmd: Command) -> Result<StateTransition, SmtpError> {
        debug!("{cmd}");
        Ok(StateTransition::from(SmtpState::CANCELLED))
    }

    async fn state_ioerror(&mut self, cmd: Command) -> Result<StateTransition, SmtpError> {
        debug!("{cmd}");
        Ok(StateTransition::from(SmtpState::CANCELLED))
    }
}