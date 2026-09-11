use std::sync::Arc;
use log::{debug};
use multimap::MultiMap;
use tokio::sync::Mutex;
use crate::config::Config;
use crate::smtp::client::SmtpClient;
use crate::smtp::envelope::{Envelope, MailAddress};
use crate::smtp::error::RemoteDeliveryError;

pub(crate) type QueueMtx = Arc<Mutex<Queue>>;

/**
Represents an envelope and its recipients.
Recipients are grouped by address domain to allow for multiple RCPT commands at their host SMTP.
*/
struct Entry {
    envelope: Envelope,
    //invariant: every inner vec consists of addresses with the same domain
    rcpt_groups: Vec<Vec<MailAddress>>
}

pub(crate) struct Queue {
    config: Config,
    entries: Vec<Entry>
}

impl Queue {
    pub(crate) fn new(config: Config) -> QueueMtx {
        Arc::new(Mutex::new(Queue {
            config,
            entries: Vec::new(),
        }))
    }

    /**
    Add `envelope` to the outgoing queue that is to be sent to `recipients`.
    `recipients` is a multimap of MailAddresses grouped by domain, so that `key == value.domain`
    holds for every entry.
    */
    pub(crate) fn add(&mut self, envelope: Envelope, mut recipients: MultiMap<&str, MailAddress>) {
        let domains: Vec<&str> = recipients.keys().map(|key| *key).collect();
        if domains.is_empty() { return }

        let mut rcpt_groups: Vec<Vec<MailAddress>> = Vec::new();
        for domain in domains {
            rcpt_groups.push(recipients.remove(domain).unwrap());
        }

        self.entries.push(Entry {
            envelope,
            rcpt_groups
        })
    }
    
    pub(crate) async fn fire(&mut self) {
        let entry = match self.entries.pop() {
            Some(entry) => entry,
            None => return
        };

        let mut errors: Vec<RemoteDeliveryError> = Vec::new();
        for recipients in entry.rcpt_groups {
            match self.deliver(&entry.envelope, recipients).await {
                Ok(_) => {},

                // todo bounce report into mailbox
                Err(err) => errors.push(err)
            }
        }

        for error in errors {
            debug!("Delivery error: {:?}", error)
        }
    }
    
    async fn deliver(&self, envelope: &Envelope, recipients: Vec<MailAddress>) -> Result<(), RemoteDeliveryError> {
        Ok(SmtpClient::deliver(self.config.clone(), envelope, recipients).await?)
    }
}