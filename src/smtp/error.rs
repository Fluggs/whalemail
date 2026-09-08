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
    NoSmtpHostError
}

impl From<ResolveError> for ClientError {
    fn from(value: ResolveError) -> Self {
        ClientError::DnsLookupError(value)
    }
}

pub(crate) enum RemoteDeliveryError {
    NoSmtpHostFound(ClientError),
    IoError(io::Error)
}

impl From<ClientError> for RemoteDeliveryError {
    fn from(value: ClientError) -> Self {
        match value {
            ClientError::NoSmtpHostError => RemoteDeliveryError::NoSmtpHostFound(ClientError::NoSmtpHostError),
            ClientError::DnsLookupError(err) => RemoteDeliveryError::NoSmtpHostFound(ClientError::DnsLookupError(err)),
            ClientError::IoError(err) => RemoteDeliveryError::IoError(err),
        }
    }
}

impl From<io::Error> for ClientError {
    fn from(value: io::Error) -> Self {
        ClientError::IoError(value)
    }
}