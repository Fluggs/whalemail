use std::io;
use std::fmt;
use strum::{Display, EnumString};
use crate::net::ConnectionHandler;

/// Used in cases where we don't know the current state to be turned into a proper SmtpError later
struct SmtpPreError {
    msg: String,    
}

#[derive(Debug, Clone)]
struct SmtpError {
    state: SmtpState,
    msg: String,
}

impl fmt::Display for SmtpError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "SMTP State {}: Unexpected message: {}", self.state, self.msg)
    }
}

impl SmtpError {
    fn new(state: SmtpState, msg: String) -> SmtpError {
        SmtpError {
            state,
            msg,
        }
    }
    
    fn from(pre_err: SmtpPreError, state: SmtpState) -> SmtpError {
        SmtpError::new(state, pre_err.msg)
    }
}

#[derive(Debug, Clone, Display, EnumString)]
enum SmtpState {
    INIT,
    HELO,
    EHLO,
    MAIL,
    RCPT,
    DATA,
    DATAINPUT,
    CANCELLED,
    IOERROR,
}

pub enum StateKind {
    KEEPGOING,
    ENDSTATE
}

pub struct Smtp<'a> {
    conn: &'a ConnectionHandler<'a>,
    pub closed: bool,
    state: SmtpState,
}

struct Command {
    verb: SmtpState,
    argstring: String
}

impl fmt::Display for Command {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{} {}", self.verb, self.argstring)
    }
}

impl Command {
    fn new(s: String) -> Result<Command, SmtpPreError> {
        let mut split = s.split(" ");
        let verb: &str = match split.next() {
            Some(r) => r,
            None => {
                println!("no next");
                return Err(SmtpPreError {
                    msg: s,
                })
            }
        };
        
        let verb = SmtpState::from(match verb.parse() {
            Ok(r) => r,
            Err(err) => {
                println!("parse failed: {}", err);
                return Err(SmtpPreError {
                    msg: s,
                })
            }
        });
        
        let r = Command {
            verb,
            argstring: split.collect::<Vec<_>>().join(" "),
        };
        
        println!("Built command '{}' from input '{}'", r, s);
        Ok(r)
    }
}

impl Smtp<'_> {
    pub fn new<'a> (connhandler: &'a ConnectionHandler) -> Smtp<'a> {
        Smtp {
            conn: connhandler,
            closed: false,
            state: SmtpState::INIT
        }
    }
    
    pub async fn init_smtp(&mut self) -> io::Result<()> {
        self.conn.send("220 hi\r\n".to_string()).await?;
        self.state = SmtpState::INIT;
        
        Ok(())
    }
    
    pub async fn handle(&mut self, input: String) -> io::Result<StateKind> {
        let cmd = match Command::new(input) {
            Ok(r) => r,
            Err(err) => {
                println!("Unexpected SMTP command message: {}", err.msg);
                return Ok(StateKind::ENDSTATE)
            }
        };
        
        let r = match self.state {
            SmtpState::INIT => self.state_init(cmd).await,
            SmtpState::EHLO => self.state_ehlo(cmd).await,
            SmtpState::HELO => self.state_helo(cmd).await,
            SmtpState::MAIL => self.state_mail(cmd).await,
            SmtpState::RCPT => self.state_rcpt(cmd).await,
            SmtpState::DATA => self.state_data(cmd).await,
            SmtpState::DATAINPUT => self.state_helo(cmd).await,
            SmtpState::CANCELLED => self.state_cancelled(cmd).await,
            SmtpState::IOERROR => self.state_ioerror(cmd).await,
        };
        
        match r {
            Ok(state) => self.state = state,
            Err(err) => {
                self.state = SmtpState::IOERROR;
                return Err(err);
            }
        }
        
        Ok(StateKind::KEEPGOING)
    }
    
    async fn state_init(&mut self, cmd: Command) -> Result<SmtpState, io::Error> {
        println!("{cmd}");
        match cmd.verb {
            SmtpState::EHLO => {
                self.conn.send("554 leave me alone\r\n".to_string()).await?;
                Ok(SmtpState::INIT)
            },
            SmtpState::HELO => {
                self.conn.send("250 OK\r\n".to_string()).await?;
                Ok(SmtpState::HELO)
            }
            _ => {
                self.conn.send("554 leave me alone\r\n".to_string()).await?;
                Ok(SmtpState::INIT)
            }
        }
    }

    async fn state_ehlo(&mut self, cmd: Command) -> Result<SmtpState, io::Error> {
        println!("{cmd}");
        Ok(SmtpState::CANCELLED)
    }

    async fn state_helo(&mut self, cmd: Command) -> Result<SmtpState, io::Error> {
        println!("{cmd}");
        match cmd.verb {
            SmtpState::MAIL => {
                self.conn.send("250 OK\r\n".to_string()).await?;
                Ok(SmtpState::MAIL)
            },
            _ => {
                self.conn.send("554 leave me alone\r\n".to_string()).await?;
                Ok(SmtpState::CANCELLED)
            }
        }
    }

    async fn state_mail(&mut self, cmd: Command) -> Result<SmtpState, io::Error> {
        println!("{cmd}");
        match cmd.verb {
            SmtpState::RCPT => {
                self.conn.send("250 OK\r\n".to_string()).await?;
                Ok(SmtpState::RCPT)
            },
            _ => {
                self.conn.send("554 leave me alone\r\n".to_string()).await?;
                Ok(SmtpState::CANCELLED)
            }
        }
    }

    async fn state_rcpt(&mut self, cmd: Command) -> Result<SmtpState, io::Error> {
        println!("{cmd}");
        match cmd.verb {
            SmtpState::DATA => {
                self.conn.send("354 start mail input\r\n".to_string()).await?;
                Ok(SmtpState::DATA)
            },
            _ => {
                self.conn.send("554 leave me alone\r\n".to_string()).await?;
                Ok(SmtpState::CANCELLED)
            }
        }
    }

    async fn state_data(&mut self, cmd: Command) -> Result<SmtpState, io::Error> {
        println!("{cmd}");
        match cmd.verb {
            SmtpState::MAIL => {
                self.conn.send("250 OK\r\n".to_string()).await?;
                Ok(SmtpState::MAIL)
            },
            _ => {
                self.conn.send("554 leave me alone\r\n".to_string()).await?;
                Ok(SmtpState::CANCELLED)
            }
        }
    }

    async fn state_cancelled(&mut self, cmd: Command) -> Result<SmtpState, io::Error> {
        println!("{cmd}");
        Ok(SmtpState::CANCELLED)
    }

    async fn state_ioerror(&mut self, cmd: Command) -> Result<SmtpState, io::Error> {
        println!("{cmd}");
        Ok(SmtpState::CANCELLED)
    }
}