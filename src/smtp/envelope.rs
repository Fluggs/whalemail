use std::fmt::{Display, Formatter};
use uuid::Uuid;
use crate::userdb::userdb::UserDBMtx;

#[derive(Debug)]
pub(crate) struct InvalidMailAddress {}

#[derive(Debug)]
#[derive(Clone)]
pub(crate) struct MailAddress {
    pub(crate) address: String,
    pub(crate) local_part: String,
    pub(crate) domain: String,
    
    // Cache for locality check against user database
    is_local_mailbox: Option<bool>,
}

impl MailAddress {
    pub(crate) fn new(s: &str) -> Result<Self, InvalidMailAddress> {
        let split: Vec<&str> = s.split("@").collect();
        match split.len() {
            2 => Ok(MailAddress {
                    address: s.to_string(),
                    local_part: split[0].to_string(),
                    domain: split[1].to_string(),
                    is_local_mailbox: None,
                }),
            _ => Err(InvalidMailAddress { })
        }
    }
    
    pub(crate) fn cached_is_local(&mut self, user_db: UserDBMtx) -> bool {
        match self.is_local_mailbox {
            Some(res) => res,
            None => {
                let r = user_db.lock().unwrap().is_local_mailbox(self);
                self.is_local_mailbox = Some(r);
                r
            }
        }
    }
    
    #[cfg(test)]
    pub(crate) fn mock() -> MailAddress {
        MailAddress {
            address: String::new(),
            local_part: String::new(),
            domain: String::new(),
            is_local_mailbox: None,
        }
    }
}

impl Display for MailAddress {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.address)
    }
}

impl PartialEq for MailAddress {
    fn eq(&self, other: &Self) -> bool {
        self.address.eq(&other.address)
    }
}

pub(crate) struct Envelope {
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
    
    #[cfg(test)]
    pub(crate) fn dummy() -> Self {
        Self {
            sender: MailAddress::mock(),
            recipients: Vec::new(),
            body: String::new(),
            uuid: Uuid::new_v4(),
        }
    }
}
