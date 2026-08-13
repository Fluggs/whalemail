use tokio::fs;
use std::{io};
use std::path::{PathBuf};
use std::time::{Instant, SystemTime};
use crate::smtp::smtp_mail::Envelope;
use log::{debug};
use crate::config::MaildirConfig;

pub(crate) struct Storage {
    hostname: String,
    user_maildir_path: String,
    base_instant: Instant,
}

impl Storage {
    pub(crate) fn new(hostname: String, maildir_config: MaildirConfig) -> Self {
        Self {
            hostname: hostname,
            user_maildir_path: maildir_config.user_maildir_path,
            base_instant: Instant::now(),
        }
    }
    
    #[cfg(test)]
    pub(crate) fn mock() -> Self {
        Self {
            hostname: String::new(),
            user_maildir_path: String::new(),
            base_instant: Instant::now(),
        }
    }
    
    /**
    Stores a mail in a mailbox identified by its name. Actual mailbox path is determined by config.
    */
    pub(crate) async fn store(&self, mail: &Envelope, mut mailbox_home: PathBuf) -> Result<(), io::Error> {
        mailbox_home.push("new");
        debug!("Writing mail '{}' to '{}'", mail.uuid, mailbox_home.display());

        fs::create_dir_all(&mailbox_home).await?;
        mailbox_home.push(self.maildir_file_name());
        fs::write(mailbox_home, &mail.body).await?;
        Ok(())
    }
    
    /**
    Constructs a new maildir file name. Format: unixtime.hostname.M[0-9]*
    Spec, loosely followed: https://cr.yp.to/proto/maildir.html
    M value is milliseconds since process start (roughly). This seems like a good approximation of the spec value.
    */
    fn maildir_file_name(&self) -> String {
        let unixtime = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH).expect("Unix time")
            .as_secs();
        let millis = Instant::now().duration_since(self.base_instant).as_millis();
        format!("{}.{}.M{}", unixtime, self.hostname, millis).to_string()
    }
}