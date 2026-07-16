use crate::smtp_error::SmtpError;
use log::{info};

pub struct SmtpTest {
    last_msg: Option<String>,
    pub received: bool,
}

impl SmtpTest {
    pub fn send(&mut self, msg: String) -> Result<(), SmtpError> {
        if !self.received && self.last_msg.is_some() {
            panic!("Last message was never received: '{}'", self.last_msg.clone().unwrap())
        }
        self.last_msg = Some(msg);
        self.received = false;
        Ok(())
    }
}

#[cfg(test)]
fn replace_newline(s: &String) -> String {
    s.replace("\n", "\\n").replace("\r", "\\r")
}

#[cfg(test)]
impl SmtpTest {
    
    fn expect_msg(&mut self, expected_msg: &str) {
        let last_msg = self.receive().unwrap();
        info!("last msg: '{}'; expectation: '{}'",
                 replace_newline(&last_msg),
                 replace_newline(&expected_msg.to_string())
        );
        assert_eq!(self.last_msg.clone(), Some(expected_msg.to_string()));
    }
    
    fn expect_no_msg(&mut self) {
        assert_eq!(self.receive(), None);
    }

    pub fn receive(&mut self) -> Option<String> {
        let r = match self.received {
            true => None,
            false => self.last_msg.clone()
        };

        self.received = true;
        r
    }
}

#[cfg(test)]
mod tests {
    use env_logger::Env;
    use crate::smtp::{Smtp, SmtpState, StateKind};
    use crate::smtp_mail::SmtpMail;
    use crate::storage::Storage;
    use crate::tests::SmtpTest;

    fn setup<'a>() -> Smtp {
        match env_logger::Builder::from_env(Env::default().default_filter_or("debug"))
            .is_test(true).try_init() {
            Ok(()) => {},
            Err(_) => {}
        };
        Smtp::new_testbed(
            SmtpTest {
                last_msg: None,
                received: false,
        }, Storage { directory: "testdir".to_string() } )
    }

    #[tokio::test]
    async fn test_init() {
        let mut s = setup();
        s.init_smtp().await.unwrap();
        let t = s.conn_testbed.as_mut().unwrap();

        t.expect_msg("220 hi\r\n");
    }

    #[tokio::test]
    async fn test_n_unknown_cmd() {
        let mut s = setup();
        s.init_smtp().await.unwrap();
        s.conn_testbed.as_mut().unwrap().expect_msg("220 hi\r\n");

        s.handle("blub\r\n".to_string()).await.unwrap();
        s.conn_testbed.as_mut().unwrap().expect_msg("500 Unrecognized command\r\n");
    }

    #[tokio::test]
    async fn test_helo_mail() {
        let sender = "sender@test.org";
        let rcpt = "rcv@whalemail.net";

        let mut s = setup();
        s.init_smtp().await.unwrap();
        s.conn_testbed.as_mut().unwrap().expect_msg("220 hi\r\n");

        s.handle("HELO test.org\r\n".to_string()).await.unwrap();
        s.conn_testbed.as_mut().unwrap().expect_msg("250 OK\r\n");

        s.handle("MAIL FROM:<".to_string() + sender + ">\r\n").await.unwrap();
        s.conn_testbed.as_mut().unwrap().expect_msg("250 OK\r\n");

        s.handle("RCPT TO:<".to_string() + rcpt + ">\r\n").await.unwrap();
        s.conn_testbed.as_mut().unwrap().expect_msg("250 OK\r\n");

        s.handle("DATA\r\n".to_string()).await.unwrap();
        s.conn_testbed.as_mut().unwrap().expect_msg("354 start mail input\r\n");

        s.handle("<mailblob> blob blob\r\n.\r\n".to_string()).await.unwrap();
        s.conn_testbed.as_mut().unwrap().expect_msg("250 OK\r\n");

        let r = s.handle("QUIT\r\n".to_string()).await.unwrap();
        s.conn_testbed.as_mut().unwrap().expect_msg("221 closing channel\r\n");
        assert_eq!(r, StateKind::QUIT);

        // Verify mail
        assert_eq!(s.mail.sender, Some(sender.to_string()));
        assert_eq!(s.mail.recipients, Vec::from([rcpt.to_string()]));
        assert!(s.mail.is_finished());
    }

    #[tokio::test]
    async fn test_ehlo_mail() {
        let sender = "sender@test.org";
        let rcpt = "rcv@whalemail.net";

        let mut s = setup();
        s.init_smtp().await.unwrap();
        s.conn_testbed.as_mut().unwrap().expect_msg("220 hi\r\n");

        s.handle("EHLO test.org\r\n".to_string()).await.unwrap();
        s.conn_testbed.as_mut().unwrap().expect_msg("502 sorry\r\n");

        s.handle("HELO test.org\r\n".to_string()).await.unwrap();
        s.conn_testbed.as_mut().unwrap().expect_msg("250 OK\r\n");

        s.handle("MAIL FROM:<".to_string() + sender + ">\r\n").await.unwrap();
        s.conn_testbed.as_mut().unwrap().expect_msg("250 OK\r\n");

        s.handle("RCPT TO:<".to_string() + rcpt + ">\r\n").await.unwrap();
        s.conn_testbed.as_mut().unwrap().expect_msg("250 OK\r\n");

        s.handle("DATA\r\n".to_string()).await.unwrap();
        s.conn_testbed.as_mut().unwrap().expect_msg("354 start mail input\r\n");

        s.handle("<mailblob> blob blob\r\n.\r\n".to_string()).await.unwrap();
        s.conn_testbed.as_mut().unwrap().expect_msg("250 OK\r\n");

        let r = s.handle("QUIT\r\n".to_string()).await.unwrap();
        s.conn_testbed.as_mut().unwrap().expect_msg("221 closing channel\r\n");
        assert_eq!(r, StateKind::QUIT);

        // Verify mail
        assert_eq!(s.mail.sender, Some(sender.to_string()));
        assert_eq!(s.mail.recipients, Vec::from([rcpt.to_string()]));
        assert!(s.mail.is_finished());
    }

    //#[tokio::test]
    async fn test_n_mail_parts() {
        let mailct_1 = "<mailblob> blob blob\r\n".to_string();
        let mailct_2 = "more blob\r\n.\r\n".to_string();
        let mut s = setup();
        s.init_smtp().await.unwrap();
        s.conn_testbed.as_mut().unwrap().expect_msg("220 hi\r\n");

        s.handle("HELO test.org\r\n".to_string()).await.unwrap();
        s.conn_testbed.as_mut().unwrap().expect_msg("250 OK\r\n");

        s.handle("MAIL FROM:<sender@test.org>\r\n".to_string()).await.unwrap();
        s.conn_testbed.as_mut().unwrap().expect_msg("250 OK\r\n");

        s.handle("RCPT TO:<rcv@whalemail.net>\r\n".to_string()).await.unwrap();
        s.conn_testbed.as_mut().unwrap().expect_msg("250 OK\r\n");

        s.handle("DATA\r\n".to_string()).await.unwrap();
        s.conn_testbed.as_mut().unwrap().expect_msg("354 start mail input\r\n");

        s.handle(mailct_1.clone()).await.unwrap();
        s.conn_testbed.as_mut().unwrap().expect_no_msg();

        s.handle(mailct_2.clone()).await.unwrap();
        s.conn_testbed.as_mut().unwrap().expect_msg("250 OK\r\n");

        let r = s.handle("QUIT\r\n".to_string()).await.unwrap();
        s.conn_testbed.as_mut().unwrap().expect_msg("221 closing channel\r\n");
        assert_eq!(r, StateKind::QUIT);

        // verify msg
        assert_eq!(s.mail.body, mailct_1 + &mailct_2);
    }

    #[tokio::test]
    async fn test_helo_multiple_rcpt() {
        let rcpt1 = "rcv1@whalemail.net";
        let rcpt2 = "rcv2@whalemail.net";
        let mut s = setup();
        println!("setup!");
        s.init_smtp().await.unwrap();
        s.conn_testbed.as_mut().unwrap().expect_msg("220 hi\r\n");

        s.handle("HELO test.org\r\n".to_string()).await.unwrap();
        s.conn_testbed.as_mut().unwrap().expect_msg("250 OK\r\n");

        s.handle("MAIL FROM:<sender@test.org>\r\n".to_string()).await.unwrap();
        s.conn_testbed.as_mut().unwrap().expect_msg("250 OK\r\n");

        s.handle("RCPT TO:<".to_string() + rcpt1 + ">\r\n").await.unwrap();
        s.conn_testbed.as_mut().unwrap().expect_msg("250 OK\r\n");

        s.handle("RCPT TO:<".to_string() + rcpt2 + ">\r\n").await.unwrap();
        s.conn_testbed.as_mut().unwrap().expect_msg("250 OK\r\n");
        
        assert_eq!(s.mail.recipients, Vec::from([rcpt1.to_string(), rcpt2.to_string()]));
    }

    #[tokio::test]
    async fn test_n_omit_rcpt() {
        let mut s = setup();
        println!("setup!");
        s.init_smtp().await.unwrap();
        s.conn_testbed.as_mut().unwrap().expect_msg("220 hi\r\n");

        s.handle("HELO test.org\r\n".to_string()).await.unwrap();
        s.conn_testbed.as_mut().unwrap().expect_msg("250 OK\r\n");

        s.handle("MAIL FROM:<sender@test.org>\r\n".to_string()).await.unwrap();
        s.conn_testbed.as_mut().unwrap().expect_msg("250 OK\r\n");

        s.handle("DATA\r\n".to_string()).await.unwrap();
        s.conn_testbed.as_mut().unwrap().expect_msg("503 Bad sequence\r\n");
    }

    /*
    Tests for decode_transparency
     */
    #[test]
    fn test_dtp_empty_s() {
        let mut smtp = setup();
        let expected = "".to_string();
        assert_eq!(smtp.decode_transparency(expected.clone()), false);
        assert_eq!(smtp.mail.body, expected);
    }
    #[test]
    fn test_dtp_empty_mail() {
        let mut smtp = setup();
        let expected = ".\r\n".to_string();
        assert_eq!(smtp.decode_transparency(expected.clone()), false);
        assert_eq!(smtp.mail.body, expected);
    }

    #[test]
    fn test_dtp_simple_mail() {
        let mut smtp = setup();
        let expected = "blub\r\n.\r\n".to_string();
        assert_eq!(smtp.decode_transparency(expected.clone()), true);
        assert_eq!(smtp.mail.body, expected);
    }

    #[test]
    fn test_dtp_first_line_transparency() {
        let mut smtp = setup();
        let input = ".\r\n.\r\n".to_string();
        let expected = "\r\n.\r\n".to_string();
        assert_eq!(smtp.decode_transparency(input), true);
        assert_eq!(smtp.mail.body, expected);
    }

    #[test]
    fn test_dtp_multi_line_transparency() {
        let mut smtp = setup();
        let input = ".abc\r\n.bcdef\r\n.\r\n".to_string();
        let expected = "abc\r\nbcdef\r\n.\r\n".to_string();
        assert_eq!(smtp.decode_transparency(input), true);
        assert_eq!(smtp.mail.body, expected);
    }

    #[test]
    fn test_dtp_double_period_transparency() {
        let mut smtp = setup();
        let input = "..a\r\n..bc\r\n.\r\n".to_string();
        let expected = ".a\r\n.bc\r\n.\r\n".to_string();
        assert_eq!(smtp.decode_transparency(input), true);
        assert_eq!(smtp.mail.body, expected);
    }

    #[test]
    fn test_dtp_multi_line_transparency_no_end() {
        let mut smtp = setup();
        let input = ".abc\r\n.bcdef\r\n".to_string();
        let expected = "abc\r\nbcdef\r\n".to_string();
        assert_eq!(smtp.decode_transparency(input), false);
        assert_eq!(smtp.mail.body, expected);
    }
    
    /*
    Tests for mail storage
     */
}
