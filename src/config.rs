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
        // Great delegation here. You may want to use the `config` crate. It supports the ini format as well.
        // Instead of using `unwrap_or_else` you can use `.map_err(|_input_err| format!("Loading {} failed", CONFIG_FILE)).unwrap())`
        // The main point being that `unwrap(panic)` is effectively the same thing as `unwrap`. The message of unwrap will be the Err value of the Result.
        // To build on this, I wouldn't want panicking to happen here. I would understand a choice to do so, but the rule of thumb is to error
        // gracefully until you want to handle a panic.
        // Crates that you will find useful for that are `anyhow` (so you can do things like `Err(e).context("Loading .. failed")?`), and
        // the crate `thiserror` in case you don't want to be creating strings with context on every error.
        // There are other patterns like having various errors map to a unified error like with `Into<JsonValue>` for API errors etc, but
        // I wouldnt expect much of that in most functions.`
        let mut conf = Ini::load_from_file(CONFIG_FILE).unwrap_or_else(|_| panic!("Loading {} failed", CONFIG_FILE));
        // This line is unclear. Why does it exist, what is it doing, do you need it?
        let conf = conf.with_section::<String>(None);
        // Obviously you are doing this as a side project, but I would expect in production to have all consts at the top
        // (where you already declared CONFIG_FILE), so it becomes easier to set, refactor, and document.
        let hostname = Hostname(conf.get("hostname").unwrap_or("whalemail.tld").to_string());
        Ok(Config {
            hostname: hostname.clone(),
            // A big issue with this is that you are having to write all this out yourself.
            // 1) Deserialising a read config file should effectively be the resulting struct.
            //    All case handling should be done with field handlers for the deserialisation.
            //    That is something specific to each crate. Often you can have macro attributes
            //    to specify what the defaults should be for a field.
            // 2) The way you use these structs means that the rust compiler won't be able to
            //    reuse the memory allocation. In this case it's not going to have any impact, but
            //    I would be mindful of thinking when the compiler can simply copy ranges of memory
            //    without checking it conditionally.
            // 3) The reason it's hard to maintain is because the fields aren't typed in a way, where
            //    defaults can be set. One way is to have the `#[default]` attribute from serdes,
            //    another is to `impl Default for MyType` where `struct MyType(String)` for example.
            bind_ip: conf.get("bind_ip").unwrap_or("127.0.0.1:25").to_string(),
            bind_ip_tls: conf.get("bind_ip_tls").unwrap_or("127.0.0.1:465").to_string(),
            cert_dir: conf.get("cert_dir").map(|s| s.to_string()),
            // The way we handle Options and Results in rust is very much through pipeing
            // `conf.get("trusted_ca_cert_dir").or(Some("/etc/ssl/certs").to_string()))`
            trusted_ca_cert_dir: Some(conf.get("trusted_ca_cert_dir").unwrap_or("/etc/sl/certs").to_string()),
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
