use uuid::Uuid;
use log::{debug};

pub(crate) struct SmtpMail {
    pub(crate) sender: Option<String>,
    pub(crate) recipients: Vec<String>,
    pub(crate) body: String,
    pub(crate) uuid: Uuid,
    finished: bool,
}

impl SmtpMail {
    pub(crate) fn new() -> SmtpMail {
        let r = SmtpMail {
            sender: None,
            recipients: Vec::new(),
            body: "".to_string(),
            uuid: Uuid::new_v4(),
            finished: false,
        };
        
        debug!("New mail with uuid {}", r.uuid);
        
        r
    }
    
    /**
    Sets a mail to finished, particularly its finished flag.
    */
    pub(crate) fn finish(&mut self) {
        self.finished = true;
    }
    
    #[cfg(test)]
    pub(crate) fn is_finished(&self) -> bool { self.finished }
}