use std::io;
use crate::config::Config;
use crate::net::{ConnectionHandler, IO};
use crate::smtp::server::StateKind;

pub(crate) struct SmtpClient<T: IO> {
    config: Config,
    conn: ConnectionHandler<T>
}

impl<T: IO> SmtpClient<T> {
    pub(crate) fn connhandler_mut(&mut self) -> &mut ConnectionHandler<T> {
        let r = &mut self.conn;
        r
    }
    
    pub(crate) async fn handle(mut self, input: String) -> Result<(Self, StateKind), io::Error> {
        todo!()
    }
}