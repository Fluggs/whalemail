use ini::Ini;

pub(crate) struct Config {
    pub(crate) log_level: String,
    pub(crate) bind_ip: String,
    pub(crate) maildir_root: String,
    cert_root: Option<String>,
}

static CONFIG_FILE: &str = "conf.ini";

impl Config {
    pub(crate) fn load() -> Config {
        let mut conf = Ini::new();
        conf.with_section::<String>(None)
            .set("bind_ip", "127.0.0.1:25")
            .set("log_level", "debug")
            .set("maildir", "maildir");
        Ini::load_from_file(CONFIG_FILE).expect(format!("Loading {} failed", CONFIG_FILE).as_str());
        let conf = conf.with_section::<String>(None);
        Config {
            log_level: conf.get("bind_ip").unwrap().to_string(),
            bind_ip: conf.get("bind_ip").unwrap().to_string(),
            maildir_root: conf.get("maildir").unwrap().to_string(),
            cert_root: conf.get("cert_directory").and_then(|s| Some(s.to_string())),
        }
    }
}