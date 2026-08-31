use std::sync::{Arc, Mutex};
use std::error;
use std::fmt::{Display, Formatter};
use crate::auth::auth::Authorized;
use crate::smtp::envelope::MailAddress;

#[derive(Debug)]
pub(crate) enum Error {
    //upstream db error
    DBError,
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl error::Error for Error {}

pub(crate) trait UserDB {
    fn authenticate(&self, authorized: &Authorized, password: String) -> Result<bool, Error>;
    fn get_mailboxhome(&self, rcpt: &MailAddress) -> Result<String, Error>;
    fn is_local_mailbox(&self, address: &MailAddress) -> bool;

    #[cfg(test)]
    fn mock_user(&mut self, username: &str, password: &str);

    #[cfg(test)]
    fn mock_mailbox(&mut self, mailbox: MailAddress);
}

pub(crate) type UserDBMtx = Arc<Mutex<dyn UserDB + Send>>;
