use std::path::PathBuf;
use rustls_pki_types::{pem, CertificateDer, PrivateKeyDer};
use rustls_pki_types::pem::PemObject;
use rustls::{RootCertStore, ServerConfig};
use std::{fs, io};
use std::fmt::{Display, Formatter};
use std::sync::Arc;
use log::debug;
use tokio_rustls::{TlsAcceptor, TlsConnector};
use crate::config::Config;

#[derive(Debug)]
#[allow(clippy::enum_variant_names)]
enum CertError {
    IoError(io::Error),
    PemError(pem::Error),
    OsError(String),
}

impl Display for CertError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            CertError::IoError(err) => write!(f, "IO error: {}", err)?,
            CertError::PemError(err) => write!(f, "PEM error: {}", err)?,
            CertError::OsError(msg) => write!(f, "{}", msg)?,
        }

        Ok(())
    }
}

impl From<io::Error> for CertError {
    fn from(err: io::Error) -> Self {
        CertError::IoError(err)
    }
}

impl From<pem::Error> for CertError {
    fn from(err: pem::Error) -> Self {
        CertError::PemError(err)
    }
}


impl CertError {
    fn new(msg: String) -> Self {
        CertError::OsError(msg)
    }
}

fn read_ca_certs(trusted_ca_cert_dir: String) -> Result<RootCertStore, CertError> {
    let mut ca_certs: Vec<CertificateDer> = Vec::new();
    for el in fs::read_dir(&trusted_ca_cert_dir)? {
        let dir_entry = &el?;
        let file_name = dir_entry.file_name()
            .to_str()
            .ok_or(CertError::new(format!("Unable to read file name from '{}'", trusted_ca_cert_dir)))?
            .to_string();

        if file_name.ends_with(".pem") {
            CertificateDer::pem_file_iter(dir_entry.path())?
                .try_for_each(|el| {
                    ca_certs.push(el?);
                    Ok::<(), pem::Error>(())
                })?;
        }
    }
    
    let mut root_cert_store = RootCertStore::empty();

    while let Some(cert) = ca_certs.pop() {
        root_cert_store.add(cert).expect("Error adding CA cert to cert store");
    }
    
    Ok(root_cert_store)
}

fn read_certs(file: impl Into<PathBuf>) -> Vec<CertificateDer<'static>> {
    let file = file.into();
    let certs: Vec<CertificateDer> = CertificateDer::pem_file_iter(file.clone())
        .unwrap_or_else(|_| panic!("Unable to read TLS certificate from {:?}", &file))
        .map(|res| res.unwrap_or_else(
            |_| panic!("Error reading certificate chain from file {:?}", file.as_os_str())
        ))
        .collect();
    
    debug!("Read TLS certificate chain from file '{}': {} certs in chain",
        &file.to_str().unwrap_or_else(|| panic!("Unable to parse file name {:?}", file.clone())),
        certs.len()
    );
    
    certs
}

pub(crate) fn build_tls_acceptor(cert_dir: String, trusted_ca_cert_dir: String) -> TlsAcceptor {
    let root_cert_store = read_ca_certs(trusted_ca_cert_dir.clone())
        .unwrap_or_else(|_| panic!("Error reading ca cert dir '{}'", trusted_ca_cert_dir));

    debug!("Root cert store: {:?}", root_cert_store);

    let mut file = PathBuf::new();
    file.push(&cert_dir);
    file.push("fullchain.pem");
    let certs = read_certs(&file);

    let mut privkey_path = PathBuf::new();
    privkey_path.push(&cert_dir);
    privkey_path.push("privkey.pem");
    println!("Trying to read privkey from {:?}", privkey_path);
    let privkey = <PrivateKeyDer as PemObject>::from_pem_file(privkey_path.clone())
        .unwrap_or_else(|_| panic!("Error reading TLS private key from file '{:?}'", privkey_path));
    println!("privkey read: {:?}", privkey);

    let server_config = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, privkey)
        .expect("Error building TLS server config");
    
    TlsAcceptor::from(Arc::new(server_config))
}

pub(crate) fn build_tls_connector(config: &Config) -> TlsConnector {
    let cert_dir = config.cert_dir.clone().expect("TLS certificate directory not configured");
    let trusted_ca_cert_dir = config.trusted_ca_cert_dir.clone().expect("TLS root cert directory not configured");
    let root_cert_store = read_ca_certs(trusted_ca_cert_dir)
        .unwrap_or_else(|err| panic!("Error reading ca cert dir: {}", err));

    debug!("Root cert store: {:?}", root_cert_store);

    let mut privkey_path = PathBuf::new();
    privkey_path.push(&cert_dir);
    privkey_path.push("privkey.pem");
    println!("Trying to read privkey from {:?}", privkey_path);
    let privkey = <PrivateKeyDer as PemObject>::from_pem_file(privkey_path.clone())
        .unwrap_or_else(|_| panic!("Error reading TLS private key from file '{:?}'", privkey_path));
    println!("privkey read: {:?}", privkey);

    let client_config = rustls::ClientConfig::builder()
        .with_root_certificates(root_cert_store)
        .with_no_client_auth();

    TlsConnector::from(Arc::new(client_config))
}
