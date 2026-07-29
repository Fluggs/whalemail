use ini::Ini;

pub(crate) struct Config {
    pub(crate) log_level: String,
    pub(crate) bind_ip: String,
    pub(crate) tls_bind_ip: String,
    pub(crate) maildir_root: String,
    pub(crate) cert_dir: Option<String>,
    pub(crate) trusted_ca_cert_dir: Option<String>,
}

static CONFIG_FILE: &str = "conf.ini";

impl Config {
    pub(crate) fn load() -> Config {
        let mut conf = Ini::load_from_file(CONFIG_FILE).expect(format!("Loading {} failed", CONFIG_FILE).as_str());
        let conf = conf.with_section::<String>(None);
        Config {
            log_level: conf.get("log_level").or(Some("debug")).unwrap().to_string(),
            bind_ip: conf.get("bind_ip").or(Some("127.0.0.1:25")).unwrap().to_string(),
            tls_bind_ip: conf.get("bind_ip").or(Some("127.0.0.1:4465")).unwrap().to_string(),
            maildir_root: conf.get("maildir").or(Some("maildir")).unwrap().to_string(),
            cert_dir: conf.get("cert_dir").and_then(|s| Some(s.to_string())),
            trusted_ca_cert_dir: Some(conf.get("trusted_ca_cert_dir").or(Some("/etc/ssl/certs")).unwrap().to_string()),
        }
    }
}