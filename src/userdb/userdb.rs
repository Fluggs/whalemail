use std::sync::{Arc, Mutex};
use crate::auth::auth::Authorized;
use crate::smtp::smtp_mail::MailAddress;

#[derive(Debug)]
pub(crate) enum Error {
    //upstream db error
    DBError,
}

pub(crate) trait UserDB {
    fn authorize(&self, authorized: &Authorized, password: String) -> bool;
    fn get_mailboxhome(&self, rcpt: &MailAddress) -> Result<String, Error>;

    #[cfg(test)]
    fn mock_user(&mut self, username: String, password: String, mailbox: String);

    #[cfg(test)]
    fn mock_mailbox(&mut self, mailbox: String);
}

pub(crate) type UserDBMtx = Arc<Mutex<dyn UserDB + Send>>;

