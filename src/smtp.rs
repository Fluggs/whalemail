use std::fmt;
use strum::{Display, EnumString};
use crate::net::ConnectionHandler;
use crate::util::string_as_bytes;
use crate::tests::SmtpTest;
use crate::smtp_error::SmtpError;
use crate::smtp_message::SmtpMessage;

#[derive(Debug, Clone, Display, EnumString, PartialEq)]
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
            state_kind: StateKind::KEEPGOING,
        }
    }
}

#[derive(Clone, Debug)]
#[derive(PartialEq)]
pub enum StateKind {
    KEEPGOING,
    ENDSTATE
}

struct Command {
    verb: Option<SmtpState>,
    remainder: String,
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
                return Err(SmtpError::bad_command(s));
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
    */
    pub async fn handle(&mut self, input: String) -> Result<StateKind, SmtpError> {
        let cmd = match Command::new(input) {
            Ok(r) => r,
            Err(err) => {
                eprintln!("Unexpected SMTP command message: '{}' ({})", err.cmd, string_as_bytes(&err.cmd));
                return Ok(StateKind::ENDSTATE)
            }
        };

        println!("Command: {}", match &cmd.verb {
            Some(v) => format!("{v}"),
            None => "<data input>".to_string()
        });
        let r = match self.state {

            // INIT -> HELO, INIT -> EHLO, FAIL -> INIT
            // todo timeout; todo avoid infinite loop INIT -> FAIL -> INIT
            SmtpState::INIT => self.expect_simple_command(cmd, Box::from([
                SimpleResponse {
                    expect: SmtpState::HELO,
                    response: "250 OK\r\n".to_string(),
                    next_state_kind: StateKind::KEEPGOING,
                },
                SimpleResponse {
                    expect: SmtpState::EHLO,
                    response: "250 OK\r\n".to_string(),
                    next_state_kind: StateKind::KEEPGOING,
                },
            ]),
            SimpleResponse {
                    expect: SmtpState::INIT,
                    response: "554 what u doing\r\n".to_string(),
                    next_state_kind: StateKind::KEEPGOING
                }
            ).await,

            // EHLO -> MAIL
            SmtpState::EHLO => self.expect_simple_command(
                cmd, Box::from([
                    SimpleResponse {
                        expect: SmtpState::MAIL,
                        response: "250 OK\r\n".to_string(),
                        next_state_kind: StateKind::KEEPGOING
                    }
                ]),
                SimpleResponse {
                    expect: SmtpState::INIT,
                    response: "554 what u doing\r\n".to_string(),
                    next_state_kind: StateKind::KEEPGOING
                }
            ).await,

            // HELO -> MAIL
            SmtpState::HELO => self.expect_simple_command(
                cmd, Box::from([
                    SimpleResponse {
                        expect: SmtpState::MAIL,
                        response: "250 OK\r\n".to_string(),
                        next_state_kind: StateKind::KEEPGOING
                    }
                ]),
                SimpleResponse {
                    expect: SmtpState::INIT,
                    response: "554 what u doing\r\n".to_string(),
                    next_state_kind: StateKind::KEEPGOING
                }
            ).await,

            // MAIL -> RCPT
            SmtpState::MAIL => self.expect_simple_command(
                cmd, Box::from([
                    SimpleResponse {
                        expect: SmtpState::RCPT,
                        response: "250 OK\r\n".to_string(),
                        next_state_kind: StateKind::KEEPGOING
                    }
                ]),
                SimpleResponse {
                    expect: SmtpState::MAIL,
                    response: "554 what u doing\r\n".to_string(),
                    next_state_kind: StateKind::KEEPGOING
                }
            ).await,

            // DATINPUT -> QUIT
            SmtpState::DATAINPUT => self.expect_simple_command(
                cmd, Box::from([
                    SimpleResponse {
                        expect: SmtpState::QUIT,
                        response: "221 closing channel\r\n".to_string(),
                        next_state_kind: StateKind::ENDSTATE
                    }
                ]),
                SimpleResponse {
                    expect: SmtpState::QUIT,
                    response: "554 what u doing\r\n".to_string(),
                    next_state_kind: StateKind::ENDSTATE
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
                println!("Transitioning: {} -> {} ({:?})", self.state, transition.next_state, transition.state_kind);
                self.state = transition.next_state;
                Ok(transition.state_kind)
            },
            Err(err) => {
                eprintln!("Error state ({})", err);
                self.state = SmtpState::IOERROR;
                Err(err)
            }
        }
    }

    async fn expect_simple_command(&mut self, cmd: Command, paths: Box<[SimpleResponse]>,
                                   fail_path: SimpleResponse) -> Result<StateTransition, SmtpError>    {
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
        println!("{cmd}");
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
                return Err(SmtpError::bad_command(format!("RCPT {}", cmd.remainder))
                    .push_state(self.state.clone())
                );
            }
        };
        
        match split.next() {
            Some(s) => {
                // Empty recipient
                if s.trim().len() == 0 {
                    return Err(SmtpError::bad_command(format!("RCPT {}", cmd.remainder))
                        .push_state(self.state.clone())
                    );
                };
                self.mail.recipients.push(s.to_string());
                Ok(())
            },
            None => Err(SmtpError::bad_command(format!("RCPT {}", cmd.remainder))
                .push_state(self.state.clone())
            )
        }
    }

    async fn state_data(&mut self, cmd: Command) -> Result<StateTransition, SmtpError> {
        println!("{cmd}");
        match cmd.verb {
            None => {
                println!("Mail!: {}", cmd.remainder);
                self.send("250 OK\r\n".to_string()).await?;
                Ok(StateTransition::from(SmtpState::DATAINPUT))
            }
            Some(v) => {
                println!("Unexpected {} after {}, expected mail input", v, self.state);
                self.send("554 leave me alone\r\n".to_string()).await?;
                Ok(StateTransition::from(SmtpState::CANCELLED))
            }
        }
    }

    async fn state_cancelled(&mut self, cmd: Command) -> Result<StateTransition, SmtpError> {
        println!("{cmd}");
        Ok(StateTransition::from(SmtpState::CANCELLED))
    }

    async fn state_ioerror(&mut self, cmd: Command) -> Result<StateTransition, SmtpError> {
        println!("{cmd}");
        Ok(StateTransition::from(SmtpState::CANCELLED))
    }
}