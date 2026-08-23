use std::fmt::{Display, Formatter};
use uuid::Uuid;
use log::{debug};

#[derive(Debug)]
pub(crate) struct InvalidMailAddress {}

#[derive(Debug)]
#[derive(PartialEq)]
pub(crate) struct MailAddress {
    pub(crate) address: String,
    pub(crate) local_part: String,
    pub(crate) domain: String,
}

impl MailAddress {
    pub(crate) fn new(s: &str) -> Result<Self, InvalidMailAddress> {
        let split: Vec<&str> = s.split("@").collect();
        match split.len() {
            2 => Ok(MailAddress {
                    address: s.to_string(),
                    local_part: split[0].to_string(),
                    domain: split[1].to_string()
                }),
            _ => Err(InvalidMailAddress { })
        }
    }
}

impl Display for MailAddress {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.address)
    }
}

pub(crate) struct Envelope {
    // todo convert to MailAddress
    pub(crate) sender: Option<String>,
    pub(crate) recipients: Vec<MailAddress>,
    pub(crate) body: String,
    pub(crate) uuid: Uuid,
    finished: bool,
}

impl Envelope {
    pub(crate) fn new2() -> Envelope {
        let r = Envelope {
            sender: None,
            recipients: Vec::new(),
            body: "".to_string(),
            uuid: Uuid::new_v4(),
            finished: false,
        };
        
        debug!("New mail with uuid {}", r.uuid);
        
        r
    }
    
    pub(crate) fn new(sender: String, recipients: Vec<MailAddress>, body: String) -> Self {
        Self {
            sender: Some(sender),
            recipients,
            body,
            uuid: Uuid::new_v4(),
            finished: true
        }
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