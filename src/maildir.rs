use tokio::fs;
use std::{io};
use std::time::{Instant, SystemTime};
use camino::Utf8PathBuf;
use crate::smtp::envelope::{Envelope, MailAddress};
use log::{debug, warn};
use crate::config::{Hostname, MaildirConfig};

#[cfg(test)] use crate::config::hostname;

pub(crate) struct Storage {
    hostname: Hostname,
    config: MaildirConfig,
    base_instant: Instant,
}

impl Storage {
    pub(crate) fn new(hostname: Hostname, maildir_config: MaildirConfig) -> Self {
        Self {
            hostname,
            config: maildir_config,
            base_instant: Instant::now(),
        }
    }
    
    #[cfg(test)]
    pub(crate) fn mock() -> Self {
        Self {
            hostname: hostname(),
            config: MaildirConfig { user_maildir_path: String::new() },
            base_instant: Instant::now(),
        }
    }

    /**
    Used variables:
    %{hostname} for the hostname from config
    %{user} for the recipient mail address
    %{mailboxhome} for mailbox home as returned by userdb
    */
    fn build_new_dir(&self, mut mailbox_home: String, recipient: &MailAddress) -> Utf8PathBuf {
        if mailbox_home.ends_with("/") {
            mailbox_home.pop();
        }
        let r = self.config.user_maildir_path
            .replace("%{mailboxhome}", mailbox_home.as_str());

        if !r.contains("%{user}") {
            warn!(target: "maildir", "Maildir configuration does not contain 'user' variable.");
        }

        let r = r.replace("%{user}", recipient.address.as_str())
            .replace("%{hostname}", self.hostname.as_str());

        let mut r = Utf8PathBuf::from(r);
        r.push("new");
        r
    }
    
    /**
    Stores a mail in a mailbox identified by its name. Actual mailbox path is determined by config.
    */
    pub(crate) async fn store(&self, mail: &Envelope, rcpt: &MailAddress, mailbox_home: String) -> Result<(), io::Error> {
        let mut mailpath = self.build_new_dir(mailbox_home, rcpt);
        debug!("Writing mail '{}' to '{}'", mail.uuid, mailpath);

        fs::create_dir_all(&mailpath).await?;
        mailpath.push(self.maildir_file_name());
        fs::write(mailpath, &mail.body).await?;
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
        format!("{}.{}.M{}", unixtime, self.hostname.as_str(), millis).to_string()
    }
}
