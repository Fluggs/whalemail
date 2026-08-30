#[derive(Clone, Debug, PartialEq)]
pub(crate) enum DeliveryError {
    NoSuchUser(String),
    MailboxIO(String)
}
