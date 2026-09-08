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

    #[tokio::test]
    async fn test_delivery() {
        let r = SmtpClient::deliver(Config::mock(), Envelope::dummy(), MailAddress::new("christian@emailgsm.de", &hostname()).unwrap()).await;
        assert!(r.is_ok())
    }
}