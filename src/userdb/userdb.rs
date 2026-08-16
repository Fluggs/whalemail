use std::path::PathBuf;
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
    fn get_mailbox_for_recipient(&self, rcpt: &MailAddress) -> Result<PathBuf, Error>;

    #[cfg(test)]
    fn mock(&mut self, username: String, password: String);
}

pub(crate) type UserDBMtx = Arc<Mutex<dyn UserDB + Send>>;

