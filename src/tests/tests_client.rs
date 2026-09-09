#[cfg(test)]
mod tests_client {
    use crate::config::{hostname, Config};
    use crate::smtp::client::SmtpClient;
    use crate::smtp::envelope::{Envelope, MailAddress};
    
    //#[tokio::test]
    async fn test_host_2() {
        let config = Config::mock();
        SmtpClient::discover_connection(&config, &MailAddress::new("mailmail@gmail.com", &hostname()).unwrap())
            .await
            .unwrap();
        //todo proper assertion
        assert!(false);
    }

    //#[tokio::test]
    async fn test_delivery() {
        let mail = Envelope::new(
            MailAddress::new("sender@whalemail.tld", &hostname()).unwrap(),
            Vec::new(),
            "testmail!".to_string()
        );
        let r = SmtpClient::deliver(Config::mock(), mail, MailAddress::new("test@test.org", &hostname()).unwrap()).await;
        assert!(r.is_ok());
        assert!(false)
    }
}