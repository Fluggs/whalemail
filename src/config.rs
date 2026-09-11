use std::fmt::{Display, Formatter};
use ini::Ini;

#[derive(Clone)]
#[derive(PartialEq)]
#[derive(Debug)]
pub(crate) struct Hostname(String);

impl PartialEq<String> for Hostname {
    fn eq(&self, other: &String) -> bool {
        self.0 == *other
    }
}

impl Display for Hostname {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Hostname {
    pub(crate) fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

#[cfg(test)]
pub(crate) fn hostname() -> Hostname {
    Hostname("whalemail.tld".to_string())
}

#[derive(Debug)]
pub(crate) enum ConfigError {
    MissingConfig(String),
}

impl Display for ConfigError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingConfig(s) => {
                Ok(write!(f, "missing config option: '{}'", s)?)
            }
        }
    }
}

#[derive(Clone)]
pub(crate) struct MaildirConfig {
    pub(crate) user_maildir_path: String,
}

#[derive(Clone)]
pub(crate) struct UserDBConfig {
    pub(crate) hostname: Hostname,
    pub(crate) postgres_database: String,
    pub(crate) postgres_username: String,
    pub(crate) postgres_password: String,
}

#[derive(Clone)]
pub(crate) struct Config {
    pub(crate) hostname: Hostname,
    pub(crate) bind_ip: String,
    pub(crate) bind_ip_tls: String,
    pub(crate) cert_dir: Option<String>,
    pub(crate) trusted_ca_cert_dir: Option<String>,
    pub(crate) log_level: String,
    pub(crate) maildir_config: MaildirConfig,
    pub(crate) userdb_config: UserDBConfig,
}

static CONFIG_FILE: &str = "conf.ini";

impl Config {
    pub(crate) fn load() -> Result<Config, ConfigError> {
        let mut conf = Ini::load_from_file(CONFIG_FILE).unwrap_or_else(|_| panic!("Loading {} failed", CONFIG_FILE));
        let conf = conf.with_section::<String>(None);
        let hostname = Hostname(conf.get("hostname").unwrap_or("whalemail.tld").to_string());
        Ok(Config {
            hostname: hostname.clone(),
            bind_ip: conf.get("bind_ip").unwrap_or("127.0.0.1:25").to_string(),
            bind_ip_tls: conf.get("bind_ip_tls").unwrap_or("127.0.0.1:465").to_string(),
            cert_dir: conf.get("cert_dir").map(|s| s.to_string()),
            trusted_ca_cert_dir: Some(conf.get("trusted_ca_cert_dir").unwrap_or("/etc/ssl/certs").to_string()),
            log_level: conf.get("log_level").unwrap_or("debug").to_string(),
            maildir_config: MaildirConfig {
                user_maildir_path: conf.get("user_maildir_path")
                    .ok_or(ConfigError::MissingConfig("user_maildir_path".to_string()))?
                    .to_string()
            },
            userdb_config: UserDBConfig {
                hostname,
                postgres_database: conf.get("postgres_database")
                    .ok_or(ConfigError::MissingConfig("postgres_username".to_string()))?
                    .to_string(),
                postgres_username: conf.get("postgres_username")
                    .ok_or(ConfigError::MissingConfig("postgres_username".to_string()))?
                    .to_string(),
                postgres_password: conf.get("postgres_password")
                    .ok_or(ConfigError::MissingConfig("postgres_password".to_string()))?
                    .to_string()
            }
        })
    }
    
    #[cfg(test)]
    pub(crate) fn mock() -> Config {
        Config {
            hostname: hostname(),
            bind_ip: "0.0.0.0:25".to_string(),
            bind_ip_tls: "0.0.0.0:465".to_string(),
            cert_dir: None,
            trusted_ca_cert_dir: None,
            log_level: "debug".to_string(),
            maildir_config: MaildirConfig { user_maildir_path: String::new() },
            userdb_config: UserDBConfig {
                hostname: hostname(),
                postgres_database: String::new(),
                postgres_username: String::new(),
                postgres_password: String::new(),
            }
        }
    }
}
