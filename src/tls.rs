use std::fs::{self, DirBuilder, OpenOptions};
use std::io::{ErrorKind, Write};
use std::net::IpAddr;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result, ensure};
use rcgen::{
    BasicConstraints, CertificateParams, DnType, ExtendedKeyUsagePurpose, IsCa, KeyPair,
    KeyUsagePurpose, SanType,
};
use rustls::ServerConfig;
use rustls::pki_types::PrivatePkcs8KeyDer;
use time::{Duration, OffsetDateTime};

pub fn configure(ip: IpAddr, directory: &Path) -> Result<Arc<ServerConfig>> {
    match DirBuilder::new().mode(0o700).create(directory) {
        Ok(()) => {}
        Err(error) if error.kind() == ErrorKind::AlreadyExists => {
            ensure!(
                fs::symlink_metadata(directory)?.is_dir(),
                "Ungültiger Zertifikatsordner"
            );
        }
        Err(error) => return Err(error).context("Zertifikatsordner erstellen"),
    }

    let key_path = directory.join("ca-key.pem");
    let cert_path = directory.join("webmic-ca.cer");
    let key = match fs::read_to_string(&key_path) {
        Ok(pem) => KeyPair::from_pem(&pem).context("Lokalen CA-Schlüssel lesen")?,
        Err(error) if error.kind() == ErrorKind::NotFound => {
            ensure!(!cert_path.exists(), "Der Schlüssel zur WebMic-CA fehlt");
            let key = KeyPair::generate()?;
            OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&key_path)?
                .write_all(key.serialize_pem().as_bytes())?;
            key
        }
        Err(error) => return Err(error).context("Lokalen CA-Schlüssel öffnen"),
    };

    let mut root = CertificateParams::default();
    root.distinguished_name
        .push(DnType::CommonName, "WebMic Local CA");
    root.is_ca = IsCa::Ca(BasicConstraints::Constrained(0));
    root.key_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign];
    let root = root.self_signed(&key)?;
    if !cert_path.exists() {
        fs::write(cert_path, root.der())?;
    }

    let mut leaf = CertificateParams::default();
    leaf.distinguished_name.push(DnType::CommonName, "WebMic");
    leaf.subject_alt_names = vec![SanType::IpAddress(ip)];
    leaf.key_usages = vec![KeyUsagePurpose::DigitalSignature];
    leaf.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
    let now = OffsetDateTime::now_utc();
    leaf.not_before = now - Duration::hours(1);
    leaf.not_after = now + Duration::days(180);
    let leaf_key = KeyPair::generate()?;
    let leaf = leaf.signed_by(&leaf_key, &root, &key)?;

    let config = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(
            vec![leaf.der().clone()],
            PrivatePkcs8KeyDer::from(leaf_key.serialize_der()).into(),
        )?;
    Ok(Arc::new(config))
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;
    use std::net::Ipv4Addr;

    use rustls::pki_types::{CertificateDer, ServerName};
    use rustls::{ClientConfig, ClientConnection, RootCertStore, ServerConnection};

    use super::*;

    #[test]
    fn saved_root_trusts_https_after_server_restarts() -> Result<()> {
        let path = std::env::temp_dir().join(format!("webmic-tls-{:016x}", rand::random::<u64>()));
        let before = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 20));
        let after = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 21));
        let result = (|| {
            configure(before, &path)?;
            let trusted = fs::read(path.join("webmic-ca.cer"))?;
            let mut roots = RootCertStore::empty();
            roots.add(CertificateDer::from(trusted.clone()))?;
            let mut client = ClientConnection::new(
                Arc::new(
                    ClientConfig::builder()
                        .with_root_certificates(roots)
                        .with_no_client_auth(),
                ),
                ServerName::IpAddress(after.into()),
            )?;
            let mut server = ServerConnection::new(configure(after, &path)?)?;

            for _ in 0..8 {
                let mut outgoing = Vec::new();
                client.write_tls(&mut outgoing)?;
                server.read_tls(&mut Cursor::new(outgoing))?;
                server.process_new_packets()?;
                let mut outgoing = Vec::new();
                server.write_tls(&mut outgoing)?;
                client.read_tls(&mut Cursor::new(outgoing))?;
                client.process_new_packets()?;
                if !client.is_handshaking() && !server.is_handshaking() {
                    assert_eq!(fs::read(path.join("webmic-ca.cer"))?, trusted);
                    return Ok(());
                }
            }
            anyhow::bail!("TLS-Handshake wurde nicht abgeschlossen");
        })();
        fs::remove_dir_all(&path)?;
        result
    }
}
