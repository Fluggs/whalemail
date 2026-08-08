use std::sync::{Arc, Mutex};
use crate::auth::auth::Authorized;


pub(crate) struct UserDB {
    // If these are set, all authorize() calls validate against this
    mock_username: Option<String>,
    mock_password: Option<String>,
}

pub(crate) type UserDBMtx = Arc<Mutex<UserDB>>; 

impl UserDB {
    pub(crate) fn new() -> Arc<Mutex<UserDB>> { 
        let r = UserDB {
            mock_username: None,
            mock_password: None,
        };
        Arc::new(Mutex::new(r))
    }
    
    #[cfg(test)]
    pub(crate) fn mock(&mut self, username: String, password: String) {
        self.mock_username = Some(username);
        self.mock_password = Some(password);
    }
    
    pub(crate) fn authorize(&self, authorized: &Authorized, password: String) -> bool {
        if self.mock_username.is_some() {
            return authorized.identity.eq(&self.mock_username.clone().unwrap())
                && password.eq(&self.mock_password.clone().unwrap());
        }
        
        false
    }
    
    pub(crate) fn get_mailboxes_for_recipients(&self, rcpts: &Vec<String>) -> Vec<String> {
        // todo match against user db
        rcpts.clone()
    }

}