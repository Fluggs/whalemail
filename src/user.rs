/**
Represents an authenticated and authorized user/identity.
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