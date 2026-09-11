use std::sync::{Arc, Mutex};
use log::debug;
use crate::auth::Authorized;
use crate::config::hostname;
use crate::smtp::envelope::MailAddress;
use crate::userdb::{Error, UserDB, UserDBMtx};

pub(crate) struct MockDB {
    pub(crate) username: String,
    pub(crate) password: String,
    pub(crate) mailbox: Option<MailAddress>,
}

impl MockDB {
    pub(crate) fn new() -> UserDBMtx {
        let r = Self {
            username: String::new(),
            password: String::new(),
            mailbox: None,
        };

        Arc::new(Mutex::new(r))
    }
}

impl UserDB for MockDB {
    fn authenticate(&self, authorized: &Authorized, password: String) -> Result<bool, Error> {
        debug!("Mock auth: '{}' wants auth for '{}'", authorized.identity, self.username);
        Ok(
            (authorized.identity.eq(&self.username) && password.eq(&self.password))
            || (self.username.eq(&(authorized.identity.clone() + "@" + hostname().as_str())) && password.eq(&self.password))
        )
    }

    fn get_mailboxhome(&self, rcpt: &MailAddress) -> Result<String, Error> {
        debug!("Getting mailboxhome; recipient: {:?}, mocked mailbox: {:?}", rcpt, self.mailbox);
        match rcpt.eq(&self.mailbox.clone().unwrap()) {
            true => Ok(String::from("testmails/%{user}")),
            false => {
                Err(Error::DBError)
            }
        }
    }

    fn is_local_mailbox(&self, address: &MailAddress) -> bool {
        let r = match self.mailbox.clone() {
            None => false,
            Some(mb) => address.eq(&mb)
        };
        
        debug!("{} is local: {}", address.address, r);
        r
    }

    fn mock_user(&mut self, username: &str, password: &str) {
        self.username = username.to_string();
        self.password = password.to_string();
        self.mailbox = None;
    }

    fn mock_mailbox(&mut self, mailbox: MailAddress) {
        self.username = String::new();
        self.password = String::new();
        self.mailbox = Some(mailbox);
    }
}
