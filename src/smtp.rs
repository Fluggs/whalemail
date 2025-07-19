use std::io;
use crate::net::ConnectionHandler;

enum SmtpState {
    INIT,
    EHLO,
    CANCELLED,
}

pub struct Smtp<'a> {
    conn: &'a ConnectionHandler<'a>,
    pub closed: bool,
    state: SmtpState,
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
        self.conn.send("220 hi".to_string()).await?;
        self.state = SmtpState::INIT;
        
        Ok(())
    }
    
    pub async fn handle(&mut self, input: String) -> io::Result<()> {
        println!("Got {input}");
        self.conn.send("554 leave me alone".to_string()).await?;
        self.state = SmtpState::CANCELLED;
        
        Ok(())
    }
}