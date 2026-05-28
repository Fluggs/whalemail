pub(crate) struct SmtpMessage {
    pub(crate) sender: Option<String>,
    pub(crate) recipients: Vec<String>,
    pub(crate) body: Option<String>
}

impl SmtpMessage {
    pub(crate) fn new() -> SmtpMessage {
        SmtpMessage {
            sender: None,
            recipients: Vec::new(),
            body: None
        }
    }
}