use crate::auth::auth::Authorized;
use crate::config::Hostname;

/**
Represents a user with its mail address, username and mailbox name.
*/
#[derive(Clone)]
#[derive(Debug)]
pub struct User {
    pub(crate) hostname: Hostname,
    pub(crate) identity: String,
    // todo work out the exact semantics of identity and username
    _username: String,
}

impl User {
    pub(crate) fn new(hostname: Hostname, identity: String, username: String) -> Self {
        Self {
            hostname,
            identity,
            _username: username
        }
    }
}

impl From<Authorized> for User {
    fn from(authorized: Authorized) -> Self {
        Self::new(authorized.hostname, authorized.identity, authorized.username)
    }
}
