use crate::auth::auth::Authorized;

/**
Represents a user with its mail address, username and mailbox name.
*/
#[derive(Clone)]
#[derive(Debug)]
pub struct User {
    pub(crate) identity: String,
    username: String,
}

impl User {
    pub(crate) fn new(identity: String, username: String) -> Self {
        Self {
            identity,
            username
        }
    }
}

impl From<Authorized> for User {
    fn from(authorized: Authorized) -> Self {
        Self::new(authorized.identity, authorized.username)
    }
}