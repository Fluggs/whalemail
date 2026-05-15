pub(crate) struct SmtpMessage {
    pub recipients: Vec<String>,
    pub(crate) body: Option<String>
}

impl SmtpMessage {
    pub(crate) fn new() -> SmtpMessage {
        SmtpMessage {
            recipients: Vec::new(),
            body: None
        }
    }
}