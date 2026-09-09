use std::io;
use hickory_resolver::ResolveError;

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum MailboxDeliveryError {
    NoSuchUser(String),
    MailboxIO(String)
}

#[derive(Debug)]
pub(crate) enum ClientError {
    DnsLookupError(ResolveError),
    IoError(io::Error),
    NoSmtpHostError,
    SmtpError(String),
    Timeout,
}

impl From<ResolveError> for ClientError {
    fn from(value: ResolveError) -> Self {
        ClientError::DnsLookupError(value)
    }
}

#[derive(Debug)]
pub(crate) enum RemoteDeliveryError {
    NoSmtpHostFound(ClientError),
    IoError(io::Error),
    SmtpError(String),
    Timeout,
}

impl From<ClientError> for RemoteDeliveryError {
    fn from(value: ClientError) -> Self {
        match value {
            ClientError::NoSmtpHostError => RemoteDeliveryError::NoSmtpHostFound(ClientError::NoSmtpHostError),
            ClientError::DnsLookupError(err) => RemoteDeliveryError::NoSmtpHostFound(ClientError::DnsLookupError(err)),
            ClientError::IoError(err) => RemoteDeliveryError::IoError(err),
            ClientError::SmtpError(msg) => RemoteDeliveryError::SmtpError(msg),
            ClientError::Timeout => RemoteDeliveryError::Timeout,
        }
    }
}

impl From<io::Error> for ClientError {
    fn from(value: io::Error) -> Self {
        ClientError::IoError(value)
    }
}