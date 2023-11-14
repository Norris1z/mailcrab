use rcgen::{Certificate, CertificateParams, DistinguishedName, DnType};
use rustls::PrivateKey;
use rustls_pemfile::Item::{Pkcs8Key, X509Certificate};
use std::sync::Arc;
use tokio::fs;
use tokio_rustls::TlsAcceptor;
use tracing::info;

use super::Result;

const CERT_PATH: &str = "cert.pem";
const KEY_PATH: &str = "key.pem";

async fn load_cert() -> Option<rustls::Certificate> {
    let pem_bytes = fs::read(CERT_PATH).await.ok()?;
    let possible_pem = rustls_pemfile::read_one_from_slice(&pem_bytes).ok()?;

    match possible_pem {
        Some((X509Certificate(_), der_bytes)) => Some(rustls::Certificate(der_bytes.to_vec())),
        _ => None,
    }
}

async fn load_key() -> Option<rustls::PrivateKey> {
    let pem_bytes = fs::read(KEY_PATH).await.ok()?;
    let possible_pem = rustls_pemfile::read_one_from_slice(&pem_bytes).ok()?;

    match possible_pem {
        Some((Pkcs8Key(inner), _)) => Some(rustls::PrivateKey(inner.secret_pkcs8_der().to_vec())),
        e => panic!("jammer {e:?}"),
    }
}

/// read or generate a certioficate + key for the SMTP server
pub(super) async fn create_tls_acceptor(name: &str) -> Result<TlsAcceptor> {
    let (cert, key) = match (load_cert().await, load_key().await) {
        (Some(c), Some(k)) => (c, k),
        _ => {
            info!("Generating self-signed certificate...");

            let mut cert_params = CertificateParams::default();
            let mut dis_name = DistinguishedName::new();
            dis_name.push(DnType::CommonName, name);
            cert_params.distinguished_name = dis_name;

            let full_cert = Certificate::from_params(cert_params).map_err(|e| e.to_string())?;
            let cert_pem = full_cert.serialize_pem().map_err(|e| e.to_string())?;

            fs::write(CERT_PATH, cert_pem)
                .await
                .map_err(|e| e.to_string())?;
            fs::write(KEY_PATH, full_cert.serialize_private_key_pem())
                .await
                .map_err(|e| e.to_string())?;

            let cert: rustls::Certificate =
                rustls::Certificate(full_cert.serialize_der().map_err(|e| e.to_string())?);
            let key = PrivateKey(full_cert.serialize_private_key_der());

            (cert, key)
        }
    };

    let config = rustls::ServerConfig::builder()
        .with_safe_defaults()
        .with_no_client_auth()
        .with_single_cert(vec![cert], key)
        .map_err(|e| e.to_string())?;

    info!("TLS configuration loaded");

    Ok(TlsAcceptor::from(Arc::new(config)))
}