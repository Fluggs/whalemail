use std::fmt::{Display, Formatter};
use log::debug;
use uuid::Uuid;
use crate::config::Hostname;
use crate::userdb::userdb::UserDBMtx;

#[cfg(test)] use crate::config::hostname;

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
    is_local_responsibility: bool,
}

impl MailAddress {
    pub(crate) fn new(s: &str, local_responsibility_hostname: &Hostname) -> Result<Self, InvalidMailAddress> {
        let split: Vec<&str> = s.split("@").collect();
        match split.len() {
            2 => {
                let domain = split[1].to_string();
                let is_local_responsibility = local_responsibility_hostname.eq(&domain);
                debug!("is local: {} == {} -> {}", local_responsibility_hostname.as_str(), domain, is_local_responsibility);
                Ok(MailAddress {
                    address: s.to_string(),
                    local_part: split[0].to_string(),
                    domain,
                    is_local_mailbox: None,
                    is_local_responsibility,
                })
            },
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

    pub(crate) fn is_local_responsibility(&self) -> bool {
        self.is_local_responsibility
    }

    #[cfg(test)]
    pub(crate) fn mock() -> MailAddress {
        MailAddress::new("mock@whalemail.tld", &hostname()).unwrap()
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

//todo rework queue so we are able to remove this clone derive
#[derive(Clone)]
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
}
