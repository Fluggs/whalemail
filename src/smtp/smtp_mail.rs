use std::fmt::{Display, Formatter};
use uuid::Uuid;

#[derive(Debug)]
pub(crate) struct InvalidMailAddress {}

#[derive(Debug)]
#[derive(PartialEq)]
#[derive(Clone)]
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
    
    #[cfg(test)]
    pub(crate) fn mock() -> MailAddress {
        MailAddress {
            address: String::new(),
            local_part: String::new(),
            domain: String::new(),
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
    pub(crate) sender: MailAddress,
    pub(crate) recipients: Vec<MailAddress>,
    pub(crate) body: String,
    pub(crate) uuid: Uuid,
}

impl Envelope {
    
    pub(crate) fn new(sender: MailAddress, recipients: Vec<MailAddress>, body: String) -> Self {
        Self {
            sender,
            recipients,
            body,
            uuid: Uuid::new_v4(),
        }
    }
}