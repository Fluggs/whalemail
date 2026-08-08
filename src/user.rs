use crate::auth::auth::Authorized;

pub(crate) enum Error {
    InvalidIdentity,
}

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
    pub(crate) fn new(identity: String, username: String) -> Result<Self, Error> {
        Ok(Self {
            identity,
            username
        })
    }
    
    pub(crate) fn mailbox_name(&self) -> String {
        self.identity.clone()
    }
}

impl From<Authorized> for User {
    fn from(authorized: Authorized) -> Self {
        Self {
            identity: authorized.identity,
            username: authorized.username,
        }
    }
}