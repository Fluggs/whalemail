use std::io;
use crate::smtp::{Smtp, StateKind};

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

fn setup<'a>() -> Smtp<'a> {
    Smtp::testing(SmtpTest {
        last_msg: None,
    })
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
