use tokio::fs;
use std::io;
use std::path::PathBuf;
use crate::smtp::smtp_mail::SmtpMail;
use log::{debug};

pub(crate) struct Storage {
    pub(crate) root_dir: String,
}

impl Storage {
    pub(crate) async fn store(&self, mail: &SmtpMail) -> Result<(), io::Error> {
        let mut file = PathBuf::new();
        file.push(self.root_dir.clone());
        debug!("Writing mail {} to {}", mail.uuid, file.display());

        fs::create_dir_all(&file).await?;
        file.push(mail.uuid.to_string());
        fs::write(file, &mail.body).await?;
        Ok(())
    }
}