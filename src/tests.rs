use crate::smtp_error::SmtpError;

pub struct SmtpTest {
    pub last_msg: Option<String>,
}

#[cfg(test)]
impl SmtpTest {
    fn replace_newline(&self, s: &String) -> String {
        s.replace("\n", "\\n").replace("\r", "\\r")
    }
    
    fn expect_msg(&self, expected_msg: &str) {
        println!("last msg: '{}'; expectation: '{}'",
                 self.replace_newline(&self.last_msg.clone().unwrap()),
                 self.replace_newline(&expected_msg.to_string())
        );
        assert_eq!(self.last_msg.clone(), Some(expected_msg.to_string()));
    }
}

impl SmtpTest {
    pub fn send(&mut self, msg: String) -> Result<(), SmtpError> {
        self.last_msg = Some(msg);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use env_logger::Env;
    use crate::smtp::{Smtp, SmtpState, StateKind};
    use crate::smtp_message::SmtpMessage;
    use crate::tests::SmtpTest;

    fn setup<'a>() -> Smtp {
        match env_logger::Builder::from_env(Env::default().default_filter_or("debug"))
            .is_test(true).try_init() {
            Ok(()) => {},
            Err(_) => {}
        };
        Smtp {
            conn: None,
            conn_testbed: Some(SmtpTest {
                last_msg: None,
            }),
            closed: false,
            state: SmtpState::INIT,
            mail: SmtpMessage {
                recipients: Vec::new(),
                body: None
            },
            last_cmd_complete: true,
            msg_buf: "".to_string(),
        }
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

        s.handle("<mailblob> blob blob\r\n".to_string()).await.unwrap();
        s.conn_testbed.as_mut().unwrap().expect_msg("250 OK\r\n");

        let r = s.handle("QUIT\r\n".to_string()).await.unwrap();
        s.conn_testbed.as_mut().unwrap().expect_msg("221 closing channel\r\n");
        assert_eq!(r, StateKind::QUIT);
    }

    #[tokio::test]
    async fn test_helo_multiple_rcpt() {
        let mut s = setup();
        println!("setup!");
        s.init_smtp().await.unwrap();
        s.conn_testbed.as_mut().unwrap().expect_msg("220 hi\r\n");

        s.handle("HELO test.org\r\n".to_string()).await.unwrap();
        s.conn_testbed.as_mut().unwrap().expect_msg("250 OK\r\n");

        s.handle("MAIL FROM:<sender@test.org>\r\n".to_string()).await.unwrap();
        s.conn_testbed.as_mut().unwrap().expect_msg("250 OK\r\n");

        s.handle("RCPT TO:<rcv1@whalemail.net>\r\n".to_string()).await.unwrap();
        s.conn_testbed.as_mut().unwrap().expect_msg("250 OK\r\n");

        s.handle("RCPT TO:<rcv2@whalemail.net>\r\n".to_string()).await.unwrap();
        s.conn_testbed.as_mut().unwrap().expect_msg("250 OK\r\n");
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
}
