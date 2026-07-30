use std::path::PathBuf;
use rustls_pki_types::{pem, CertificateDer, PrivateKeyDer};
use rustls_pki_types::pem::PemObject;
use rustls::{RootCertStore, ServerConfig};
use std::{fs, io};
use std::sync::Arc;
use log::debug;
use tokio_rustls::{TlsAcceptor, TlsConnector};

#[derive(Debug)]
struct CertErr {
    pub(crate) message: Option<String>,
    pub(crate) from_io_err: Option<io::Error>,
    pub(crate) from_pem_err: Option<pem::Error>,
}

impl From<io::Error> for CertErr {
    fn from(err: io::Error) -> Self {
        Self {
            message: None,
            from_io_err: Some(err),
            from_pem_err: None,
        }
    }
}

impl From<pem::Error> for CertErr {
    fn from(err: pem::Error) -> Self {
        Self {
            message: None,
            from_io_err: None,
            from_pem_err: Some(err),
        }
    }
}


impl CertErr {
    fn new(message: String) -> Self {
        Self {
            message: Some(message),
            from_io_err: None,
            from_pem_err: None,
        }
    }
}

fn read_ca_certs<'a>(trusted_ca_cert_dir: String) -> Result<RootCertStore, CertErr> {
    let mut ca_certs: Vec<CertificateDer> = Vec::new();
    for el in fs::read_dir(&trusted_ca_cert_dir)? {
        let dir_entry = &el?;
        let file_name = dir_entry.file_name()
            .to_str()
            .ok_or(CertErr::new(format!("Unable to read file name from '{}'", trusted_ca_cert_dir)))?
            .to_string();

        match file_name.ends_with(".pem") {
            true => {
                CertificateDer::pem_file_iter(dir_entry.path())?
                    .try_for_each(|el| {
                        ca_certs.push(el?);
                        Ok::<(), pem::Error>(())
                    })?;
            },
            false => {}
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
        .unwrap()
        .map(|res| res.expect(
            format!("Error reading certificate chain from file {:?}", file.as_os_str()).as_str()
        ))
        .collect();
    
    debug!("Read TLS certificate chain from file '{}': {} certs in chain",
        &file.to_str().expect(format!("Unable to parse file name {:?}", file.clone()).as_str()),
        certs.len()
    );
    
    certs
}

pub(crate) fn build_tls_acceptor(cert_dir: String, trusted_ca_cert_dir: String) -> TlsAcceptor {
    let root_cert_store = read_ca_certs(trusted_ca_cert_dir.clone())
        .expect(format!("Error reading ca cert dir '{}'", trusted_ca_cert_dir).as_str());

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
        .expect(format!("Error reading TLS private key from file '{:?}'", privkey_path).as_str());
    println!("privkey read: {:?}", privkey);

    let server_config = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, privkey)
        .expect("Error building TLS server config");
    
    TlsAcceptor::from(Arc::new(server_config))
}

pub(crate) fn build_tls_connector(cert_dir: String, trusted_ca_cert_dir: String) -> TlsConnector {

    let root_cert_store = read_ca_certs(trusted_ca_cert_dir).expect("Error reading ca cert dir");

    debug!("Root cert store: {:?}", root_cert_store);

    let mut privkey_path = PathBuf::new();
    privkey_path.push(&cert_dir);
    privkey_path.push("privkey.pem");
    println!("Trying to read privkey from {:?}", privkey_path);
    let privkey = <PrivateKeyDer as PemObject>::from_pem_file(privkey_path.clone())
        .expect(format!("Error reading TLS private key from file '{:?}'", privkey_path).as_str());
    println!("privkey read: {:?}", privkey);

    let client_config = rustls::ClientConfig::builder()
        .with_root_certificates(root_cert_store)
        .with_no_client_auth();

    TlsConnector::from(Arc::new(client_config))
}