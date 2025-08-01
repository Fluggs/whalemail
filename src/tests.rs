use std::io;
use crate::smtp::{Smtp, StateKind};

pub struct SmtpTest{
    pub last_msg: String,
}

impl SmtpTest {
    pub fn send(&mut self, msg: &String) -> io::Result<()> {
        self.last_msg = msg.clone();
        Ok(())
    }
    
    pub fn get_latest_msg(&mut self) -> &String {
        &self.last_msg
    }
}