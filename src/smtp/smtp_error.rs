use std::fmt;
use std::io::Error;
use crate::smtp::smtp::{SmtpState, Command};

#[derive(Debug, Clone)]
#[derive(PartialEq)]
pub(crate) enum ErrorKind {
    BADCOMMAND,
    BADSEQUENCE,
    BADAUTHMECH,
    BADCREDENTIALS,
    IOERROR,
}

#[derive(Debug)]
pub(crate) struct SmtpError {
    pub(crate) kind: ErrorKind,

    /// The state the state machine is in
    pub(crate) state: Option<SmtpState>,

    /// The smtp message that caused this error
    pub(crate) cmd: String,
    
    ///Potential original IO error
    pub(crate) io_error: Option<Error>,
}

impl fmt::Display for SmtpError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let state = match &self.state {
            Some(s) => format!("{}", s),
            None => "(unknown)".to_string()
        };
        write!(f, "SMTP State {}: Unexpected message: {}", state, self.cmd)
    }
}

impl Into<Error> for SmtpError {
    fn into(self) -> Error {
        assert_eq!(self.kind, ErrorKind::IOERROR);
        self.io_error.unwrap()
    }
}

impl SmtpError {
    pub(crate) fn bad_command(cmd: &Command) -> SmtpError {
        SmtpError {
            kind: ErrorKind::BADCOMMAND,
            state: None,
            cmd: format!("{}", cmd.message),
            io_error: None,
        }
    }
    
    pub(crate) fn bad_sequence(cmd: &Command, state: SmtpState) -> SmtpError {
        SmtpError {
            kind: ErrorKind::BADSEQUENCE,
            state: Some(state),
            cmd: format!("{}", cmd.message),
            io_error: None,
        }
    }

    pub(crate) fn bad_auth_mech(cmd: &Command, state: SmtpState) -> SmtpError {
        SmtpError {
            kind: ErrorKind::BADAUTHMECH,
            state: Some(state),
            cmd: format!("{}", cmd.message),
            io_error: None,
        }
    }

    pub(crate) fn bad_credentials(cmd: &Command, state: SmtpState) -> SmtpError {
        SmtpError {
            kind: ErrorKind::BADCREDENTIALS,
            state: Some(state),
            cmd: format!("{}", cmd.message),
            io_error: None,
        }
    }

    pub(crate) fn from_io(io_err: Error) -> SmtpError {
        SmtpError {
            kind: ErrorKind::IOERROR,
            state: None,
            cmd: "io error".to_string(),
            io_error: Some(io_err),
        }
    }
    
    pub(crate) fn push_state(mut self, state: SmtpState) -> SmtpError {
        self.state = Some(state);
        self
    }
}