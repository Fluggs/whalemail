use std::sync::Arc;
use log::debug;
use tokio::sync::Mutex;
use crate::config::Config;
use crate::smtp::client::SmtpClient;
use crate::smtp::envelope::{Envelope, MailAddress};
use crate::smtp::error::RemoteDeliveryError;

pub(crate) struct Entry {
    pub(crate) envelope: Envelope,
    pub(crate) recipient: MailAddress,
}

pub(crate) type QueueMtx = Arc<Mutex<Queue>>;

pub(crate) struct Queue {
    config: Config,
    entries: Vec<Entry>,
}

impl Queue {
    pub(crate) fn new(config: Config) -> QueueMtx {
        Arc::new(Mutex::new(Queue {
            config,
            entries: Vec::new(),
        }))
    }
    
    pub(crate) fn add(&mut self, envelope: &Envelope, recipient: MailAddress) {
        debug!("Adding  mail for recipient '{}' to outgoing queue", recipient);
        let envelope = envelope.clone();
        self.entries.push(Entry {
            envelope,
            recipient,
        })
    }
    
    pub(crate) async fn fire(&mut self) {
        let entry = match self.entries.pop() {
            Some(entry) => entry,
            None => return
        };

        // todo bounce report
        let _ = self.deliver(entry);
    }
    
    async fn deliver(&self, entry: Entry) -> Result<(), RemoteDeliveryError> {
        Ok(SmtpClient::deliver(self.config.clone(), entry.envelope, entry.recipient).await?)
    }

    #[cfg(test)]
    pub(crate) fn pop(&mut self) -> Option<Entry> {
        self.entries.pop()
    }
}