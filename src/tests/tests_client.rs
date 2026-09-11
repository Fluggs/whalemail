#[cfg(test)]
mod tests_client {
    use crate::config::{hostname, Config};
    use crate::smtp::client::SmtpClient;
    use crate::smtp::envelope::{Envelope, MailAddress};
    
    //#[tokio::test]
    async fn test_host_2() {
        let config = Config::mock();
        SmtpClient::discover_connection(&config, "gmail.com")
            .await
            .unwrap();
        //todo proper assertion
        assert!(false);
    }

    //#[tokio::test]
    async fn test_delivery() {
        let mut mail = Envelope::new(
            MailAddress::new("sender@whalemail.tld", &hostname()).unwrap(),
            Vec::new(),
            "testmail!".to_string()
        );
        let rcpt = Vec::from([MailAddress::new("test@test.org", &hostname()).unwrap()]);
        let r = SmtpClient::deliver(Config::mock(), &mut mail, rcpt).await;
        assert!(r.is_ok());
        assert!(false)
    }
}