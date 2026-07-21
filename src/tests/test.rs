use crate::smtp::smtp_error::SmtpError;

/*

        let last_msg = self.receive().unwrap_or_else(|| {
            assert!(false, "Expected a message to be received, got None instead");
            String::new()
        });
        info!("last msg: '{}'; expectation: '{}'",
                 replace_newline(&last_msg),
                 replace_newline(&expected_msg.to_string())
        );
        assert_eq!(last_msg.clone(), expected_msg.to_string(), "{}", format!("Expected to receive message {}, got {} instead", last_msg, expected_msg.to_string()));
 */

#[cfg(test)]
macro_rules! expect_msg {
    ($s:expr, $x:expr) => (
        let last_msg = $s.conn_testbed.as_mut().unwrap().receive().unwrap_or("<None>".to_string());
        assert_eq!(
            last_msg.clone(),
            $x.to_string(),
            "Expected to receive message '{}', got '{}' instead",
            &last_msg.replace("\n", "\\n").replace("\r", "\\r"),
            &$x.to_string().replace("\n", "\\n").replace("\r", "\\r")
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
    use crate::auth::userdb::UserDB;
    use crate::smtp::smtp::Smtp;
    use crate::storage::Storage;
    use crate::tests::test::{SmtpTest};


    #[cfg(test)]
    pub(crate) fn test_setup() -> Smtp {
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
            UserDB::new(),
            Storage { directory: "testdir".to_string() }
        )
    }

}
