use std::fmt::{Display, Formatter};
use ini::Ini;

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
    pub(crate) postgres_database: String,
    pub(crate) postgres_username: String,
    pub(crate) postgres_password: String,
}

#[derive(Clone)]
pub(crate) struct Config {
    pub(crate) hostname: String,
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
        let mut conf = Ini::load_from_file(CONFIG_FILE).expect(format!("Loading {} failed", CONFIG_FILE).as_str());
        let conf = conf.with_section::<String>(None);
        Ok(Config {
            hostname: conf.get("hostname").or(Some("whalemail.tld")).unwrap().to_string(),
            bind_ip: conf.get("bind_ip").or(Some("127.0.0.1:25")).unwrap().to_string(),
            bind_ip_tls: conf.get("bind_ip_tls").or(Some("127.0.0.1:465")).unwrap().to_string(),
            cert_dir: conf.get("cert_dir").and_then(|s| Some(s.to_string())),
            trusted_ca_cert_dir: Some(conf.get("trusted_ca_cert_dir").or(Some("/etc/ssl/certs")).unwrap().to_string()),
            log_level: conf.get("log_level").or(Some("debug")).unwrap().to_string(),
            maildir_config: MaildirConfig {
                user_maildir_path: conf.get("user_maildir_path")
                    .ok_or(ConfigError::MissingConfig("user_maildir_path".to_string()))?
                    .to_string()
            },
            userdb_config: UserDBConfig {
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
            hostname: "whalemail.tld".to_string(),
            bind_ip: "0.0.0.0:25".to_string(),
            bind_ip_tls: "0.0.0.0:465".to_string(),
            cert_dir: None,
            trusted_ca_cert_dir: None,
            log_level: "debug".to_string(),
            maildir_config: MaildirConfig { user_maildir_path: String::new() },
            userdb_config: UserDBConfig {
                postgres_database: String::new(),
                postgres_username: String::new(),
                postgres_password: String::new(),
            }
        }
    }
}