use std::io;
use crate::smtp::{Mail, Smtp, SmtpState, StateKind};

pub struct SmtpTest {
    pub last_msg: Option<String>,
}

impl SmtpTest {
    pub(crate) fn expect_msg(&self, msg: &str) {
        println!("last msg: '{}'; expectation: '{}'", self.last_msg.clone().unwrap(), msg);
        assert_eq!(self.last_msg.clone(), Some(msg.to_string()));
    }
}

impl SmtpTest {
    pub fn send(&mut self, msg: String) -> io::Result<()> {
        self.last_msg = Some(msg);
        Ok(())
    }
}

#[cfg(test)]

fn setup<'a>() -> Smtp {
    Smtp {
        conn: None,
        conn_testbed: Some(SmtpTest {
            last_msg: None,
        }),
        closed: false,
        state: SmtpState::INIT,
        mail: Mail {
            recipients: Vec::new(),
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
async fn test_unknown_cmd() {
    let mut s = setup();
    s.init_smtp().await.unwrap();
    s.conn_testbed.as_mut().unwrap().expect_msg("220 hi\r\n");

    s.handle("blub\r\n".to_string()).await.unwrap();
    s.conn_testbed.as_mut().unwrap().expect_msg("554 what u doing\r\n");
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
    assert_eq!(r, StateKind::ENDSTATE);
}

#[tokio::test]
async fn test_helo_multiple_rcpt() {
    let mut s = setup();
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
