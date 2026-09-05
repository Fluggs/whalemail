#[cfg(test)]
mod tests_client {
    use tokio::net::TcpStream;
    use crate::config::Config;
    use crate::smtp::client::SmtpClient;
    use crate::smtp::envelope::{Envelope, MailAddress};
    
    #[tokio::test]
    async fn test_host_2() {
        let config = Config::mock();
        SmtpClient::<TcpStream>::build_connection(&config, Envelope::dummy(), MailAddress::new("mailmail@gmail.com").unwrap())
            .await
            .unwrap();
        assert!(false);
    }
}