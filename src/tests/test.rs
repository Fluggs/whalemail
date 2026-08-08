use crate::smtp::smtp_error::SmtpError;

#[cfg(test)]
macro_rules! expect_msg {
    ($s:expr, $x:expr) => (
        let last_msg = $s.conn_writer.conn_testbed.as_mut().unwrap().receive().unwrap_or("<None>".to_string());
        assert_eq!(
            last_msg.clone(),
            $x.to_string(),
            "Expected to receive message '{}', got '{}' instead",
            &$x.to_string().replace("\n", "\\n").replace("\r", "\\r"),
            &last_msg.replace("\n", "\\n").replace("\r", "\\r"),
        )
    );
}

#[cfg(test)]
pub(crate) use expect_msg;

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
impl SmtpTest {
    
    pub(crate) fn expect_no_msg(&mut self) {
        assert_eq!(self.receive(), None);
    }

    pub fn receive(&mut self) -> Option<String> {
        match self.received {
            true => None,
            false => {
                self.received = true;
                self.last_msg.clone()
            }
        }
    }
}

#[cfg(test)]
pub mod test {
    use env_logger::Env;
    use tokio::net::TcpStream;
    use crate::auth::userdb::UserDB;
    use crate::config::Config;
    use crate::net::IO;
    use crate::smtp::smtp::Smtp;
    use crate::maildir::Storage;
    use crate::tests::test::{SmtpTest};
    
    pub(crate) fn ehlo_msg(s: &Smtp<TcpStream>) -> String {
        Smtp::<TcpStream>::ehlo_response(s.config_ref())
    }

    #[cfg(test)]
    pub(crate) fn test_setup<T: IO>() -> Smtp<T> {
        match env_logger::Builder::from_env(Env::default().default_filter_or("debug"))
            .is_test(true).try_init() {
            Ok(()) => {},
            Err(_) => {}
        };
        Smtp::new_testbed(
            SmtpTest {
                last_msg: None,
                received: false,
            },
            Config::mock(),
            UserDB::new(),
            Storage::mock()
        )
    }

}
