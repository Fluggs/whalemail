pub(crate) struct SmtpMail {
    pub(crate) sender: Option<String>,
    pub(crate) recipients: Vec<String>,
    pub(crate) body: String,
    #[allow(dead_code)]
    guard: bool,
}

impl SmtpMail {
    pub(crate) fn new() -> SmtpMail {
        SmtpMail {
            sender: None,
            recipients: Vec::new(),
            body: "".to_string(),
            guard: true,
        }
    }
}