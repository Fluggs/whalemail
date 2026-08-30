use std::sync::{Arc, Mutex};
use crate::auth::auth::Authorized;
use crate::smtp::envelope::MailAddress;
use crate::userdb::userdb::{Error, UserDB, UserDBMtx};

pub(crate) struct MockDB {
    pub(crate) username: String,
    pub(crate) password: String,
    pub(crate) mailbox: String,
}

impl MockDB {
    pub(crate) fn new() -> UserDBMtx {
        let r = Self {
            username: String::new(),
            password: String::new(),
            mailbox: String::new(),
        };

        Arc::new(Mutex::new(r))
    }
}

impl UserDB for MockDB {
    fn authenticate(&self, authorized: &Authorized, password: String) -> Result<bool, Error> {
        Ok(authorized.identity.eq(&self.username.clone())
            && password.eq(&self.password.clone()))
    }

    fn get_mailboxhome(&self, rcpt: &MailAddress) -> Result<String, Error> {
        match rcpt.address.eq(&self.mailbox) {
            true => Ok(String::from("testmails/%{user}")),
            false => Err(Error::DBError)
        }
    }

    fn mock_user(&mut self, username: String, password: String, mailbox: String) {
        self.username = username;
        self.password = password;
        self.mailbox = mailbox;
    }

    fn mock_mailbox(&mut self, mailbox: String) {
        self.username = String::new();
        self.password = String::new();
        self.mailbox = mailbox;
    }
}
