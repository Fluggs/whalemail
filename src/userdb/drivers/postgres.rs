use std::sync::{Arc, Mutex};
use log::{debug, error};
use tokio_postgres::{Client, NoTls};
use crate::auth::auth::Authorized;
use crate::config::UserDBConfig;
use crate::smtp::smtp_mail::MailAddress;
use crate::userdb::userdb::{Error, UserDBMtx, UserDB};

pub(crate) struct Postgres {
    // If these are set, all authorize() calls validate against this
    client: Client,
}

impl Postgres {
    pub(crate) async fn new(config: UserDBConfig) -> Result<UserDBMtx, Error> {
        let (client, connection) = match
        tokio_postgres::connect(format!("host=localhost user={} password={} dbname={}",
                                        config.postgres_username,
                                        config.postgres_password,
                                        config.postgres_database).as_str(),
                                NoTls)
            .await {
            Ok(r) => Ok(r),
            Err(err) => {
                error!("Error connecting to user database: '{:?}', '{:?}', '{:?}'", err, err.code(), err.as_db_error());
                Err(Error::DBError)
            }
        }?;
        let r = Postgres {
            client,
        };

        tokio::spawn(async move {
            if let Err(e) = connection.await {
                error!("Postgres connection error: '{}'", e);
            }
        });


        Ok(Arc::new(Mutex::new(r)))
    }
}

impl UserDB for Postgres {
    fn authorize(&self, authorized: &Authorized, password: String) -> bool {
        todo!()
    }

    /**
    Takes a recipient and retrieves its mailbox name from the user db
    */
    fn get_mailboxhome(&self, rcpt: &MailAddress) -> Result<String, Error> {
        let runtime = tokio::runtime::Handle::current();
        let rows = tokio::task::block_in_place(move ||
            runtime.block_on(self.client
            .query("SELECT home AS mailbox_home FROM users WHERE username = $1::TEXT AND domain = $2::TEXT;",
                   &[&rcpt.local_part, &rcpt.domain])
        ))
            .or(Err(Error::DBError))?;

        let row = match rows.len() {
            0 => {
                debug!("No mailbox found for user '{}'", rcpt);
                return Err(Error::DBError);
            },
            1 => {
                rows.get(0).unwrap()
            },
            p => {
                error!("Expected at most one mailbox, retrieved {} from DB for address '{}'", p, rcpt.address);
                return Err(Error::DBError);
            }
        };

        let r = row.get::<&str, String>("mailbox_home");
        debug!("Found mailbox home for recipient: '{:?}' for '{}'", r, rcpt.address);

        Ok(r)
    }

    #[cfg(test)]
    fn mock_user(&mut self, _username: String, _password: String, _mailbox: String) {
        todo!()
    }

    #[cfg(test)]
    fn mock_mailbox(&mut self, _mailbox: String) {
        todo!()
    }
}