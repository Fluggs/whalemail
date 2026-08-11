use std::sync::{Arc, Mutex};
use log::{debug, error};
use crate::auth::auth::Authorized;
use tokio_postgres::{NoTls, Client, Connection, Socket};
use tokio_postgres::tls::NoTlsStream;
use crate::config::UserDBConfig;
use crate::smtp::smtp_mail::MailAddress;

#[derive(Debug)]
pub(crate) enum Error {
    //upstream db error
    DBError,
}

pub(crate) struct UserDB {
    // If these are set, all authorize() calls validate against this
    mock_username: Option<String>,
    mock_password: Option<String>,
    postgres: Option<Postgres>,
}

struct Postgres {
    client: Client,
    connection: Connection<Socket, NoTlsStream>
}

pub(crate) type UserDBMtx = Arc<Mutex<UserDB>>;

impl UserDB {
    pub(crate) async fn new(config: UserDBConfig) -> Result<UserDBMtx, Error> {
        let (client, connection) = match
            tokio_postgres::connect(format!("host=localhost user={} password={}",
                                            config.postgres_username,
                                            config.postgres_password).as_str(),
                                    NoTls)
                .await {
            Ok(r) => Ok(r),
            Err(err) => {
                error!("Error connecting to user database: '{:?}', '{:?}', '{:?}'", err, err.code(), err.as_db_error());
                Err(Error::DBError)
            }
        }?;
        let r = UserDB {
            mock_username: None,
            mock_password: None,
            postgres: Some(Postgres {
                client,
                connection,
            })
        };
        Ok(Arc::new(Mutex::new(r)))
    }

    #[cfg(test)]
    pub(crate) fn new_mock() -> UserDBMtx {
        todo!()
    }
    
    #[cfg(test)]
    pub(crate) fn mock(&mut self, username: String, password: String) {
        self.mock_username = Some(username);
        self.mock_password = Some(password);
    }

    fn client(&self) -> &Client {
        let p = &self.postgres.as_ref();
        &p.unwrap().client
    }
    
    pub(crate) fn authorize(&self, authorized: &Authorized, password: String) -> bool {
        if self.mock_username.is_some() {
            return authorized.identity.eq(&self.mock_username.clone().unwrap())
                && password.eq(&self.mock_password.clone().unwrap());
        }
        
        false
    }
    
    /**
    Takes a recipient and retrieves its mailbox name from the user db
    */
    pub(crate) async fn get_mailbox_for_recipient(&self, rcpt: &MailAddress) -> Result<String, Error> {
        let rows = self.client()
            .query("SELECT home AS mailbox_home FROM users WHERE username = $1::TEXT AND domain = $2::TEXT;", &[&"hello world"])
            .await.or(Err(Error::DBError))?;
        // todo match against user db
        debug!("{:?}", rows);
        let row = match rows.len() {
            0 => {
                debug!("No mailbox found for user {}", rcpt);
                return Err(Error::DBError);
            },
            1 => {
                rows.get(0).unwrap()
            },
            p => {
                debug!("Expected one or no mailboxes, retrieved {} from db", p);
                return Err(Error::DBError);
            }
        };
        
        let r: String = row.get("mailbox_home");
        debug!("suspecting mailbox: '{}'", r);
        
        Ok("mock_mailbox".to_string())
    }

}