//! The hub's TLS certificate for paired devices. It's made the first time one is needed and kept, because each
//! device pins its key: a new one means pairing every device again.

use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use tokio_rustls::TlsAcceptor;
use tokio_rustls::rustls::ServerConfig;
use tokio_rustls::rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use tracing::{info, warn};

/// What the hub needs to serve TLS, and what a device pins.
pub(crate) struct Identity {
    /// See [`fingerprint`].
    pub(crate) fingerprint: String,
    pub(crate) acceptor: TlsAcceptor,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum CertificateError {
    #[error("{what}")]
    Io { what: String, source: std::io::Error },
    /// What the hub kept can't be used. It's left alone, since replacing it would unpair every device.
    #[error(
        "{} can't be used. Moving the hub's certificate and key aside makes new ones, and then every device has to pair again",
        path.display()
    )]
    Kept { path: PathBuf, source: Box<CertificateError> },
    #[error("making a certificate")]
    Make(#[from] rcgen::Error),
    #[error("reading the certificate")]
    Read(#[from] webpki::Error),
    #[error("setting up TLS")]
    Tls(#[from] tokio_rustls::rustls::Error),
}

/// The SHA-256 of a certificate's `SubjectPublicKeyInfo`, in unpadded base64url, which is what a pairing link
/// carries. It's the key that's pinned rather than the whole certificate, so the same key could be certified again.
/// `client_core` works it out the same way from what a hub presents, and pairing fails if the two ever differ.
pub(crate) fn fingerprint(certificate: &CertificateDer<'_>) -> Result<String, webpki::Error> {
    let key = webpki::EndEntityCert::try_from(certificate)?.subject_public_key_info();
    Ok(URL_SAFE_NO_PAD.encode(ring::digest::digest(&ring::digest::SHA256, key.as_ref())))
}

/// The certificate and key kept at these paths, or new ones if there's no key yet. Blocks.
///
/// Only a key that isn't there is made anew. Any other trouble, such as a file this user can't read, is an error
/// rather than a reason to replace the key, because a new key unpairs every device, and the trouble is better fixed.
pub(crate) fn load_or_create(certificate: &Path, key: &Path) -> Result<Identity, CertificateError> {
    let Some(key_der) = read_if_there(key)? else {
        let key_pair = rcgen::KeyPair::generate()?;
        let (certificate_der, key_der) = (certify(&key_pair)?, key_pair.serialize_der());
        // The key first, so a certificate on disk always has its key beside it.
        write_private(key, &key_der)?;
        write_private(certificate, &certificate_der)?;
        let identity = identity(certificate_der, key_der)?;
        info!(fingerprint = identity.fingerprint, "made a certificate for paired devices");
        return Ok(identity);
    };
    let unusable =
        |path: &Path, e: CertificateError| CertificateError::Kept { path: path.to_path_buf(), source: Box::new(e) };
    let key_pair = rcgen::KeyPair::try_from(key_der.as_slice()).map_err(|e| unusable(key, e.into()))?;
    let certificate_der = if let Some(certificate_der) = read_if_there(certificate)? {
        certificate_der
    } else {
        // Certifying the same key again keeps its fingerprint, so no device has to pair again. A hub stopped between
        // writing the two files leaves this.
        let certificate_der = certify(&key_pair)?;
        write_private(certificate, &certificate_der)?;
        warn!("made a new certificate for the hub's key, which devices already pinned");
        certificate_der
    };
    // The key read, so what's wrong is the certificate, or that it's for another key.
    identity(certificate_der, key_der).map_err(|e| unusable(certificate, e))
}

/// What's at `path`, or `None` if there's nothing there.
fn read_if_there(path: &Path) -> Result<Option<Vec<u8>>, CertificateError> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(CertificateError::Io { what: format!("reading {}", path.display()), source }),
    }
}

/// A self-signed certificate for `key`, as DER. Nothing checks its names or dates, because devices pin its key
/// instead.
fn certify(key: &rcgen::KeyPair) -> Result<Vec<u8>, CertificateError> {
    let mut params = rcgen::CertificateParams::new(Vec::<String>::new())?;
    params.distinguished_name.push(rcgen::DnType::CommonName, "Roundhouse hub");
    Ok(params.self_signed(key)?.der().to_vec())
}

/// TLS 1.3 only, since every device is a build of client-core.
fn identity(certificate: Vec<u8>, key: Vec<u8>) -> Result<Identity, CertificateError> {
    let certificate = CertificateDer::from(certificate);
    let fingerprint = fingerprint(&certificate)?;
    let key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key));
    let provider = Arc::new(tokio_rustls::rustls::crypto::ring::default_provider());
    let config = ServerConfig::builder_with_provider(provider)
        .with_protocol_versions(&[&tokio_rustls::rustls::version::TLS13])?
        .with_no_client_auth()
        .with_single_cert(vec![certificate], key)?;
    Ok(Identity { fingerprint, acceptor: TlsAcceptor::from(Arc::new(config)) })
}

/// Written beside its path and moved into place, so a crash can't leave half a file, with only this user able to
/// read it.
fn write_private(path: &Path, bytes: &[u8]) -> Result<(), CertificateError> {
    let io = |what: &str| {
        let what = format!("{what} {}", path.display());
        move |source| CertificateError::Io { what, source }
    };
    let partial = path.with_extension("new");
    let _ = std::fs::remove_file(&partial);
    let mut file =
        std::fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(&partial).map_err(io("creating"))?;
    file.write_all(bytes).and_then(|()| file.sync_all()).map_err(io("writing"))?;
    std::fs::rename(&partial, path).map_err(io("saving"))
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt;

    use super::*;

    #[test]
    fn a_certificate_is_made_once_and_only_this_user_can_read_it() {
        let dir = tempfile::tempdir().unwrap();
        let (certificate, key) = (dir.path().join("hub-cert.der"), dir.path().join("hub-key.der"));

        let made = load_or_create(&certificate, &key).unwrap();
        let kept = load_or_create(&certificate, &key).unwrap();

        assert_eq!(made.fingerprint, kept.fingerprint);
        for file in [&certificate, &key] {
            assert_eq!(std::fs::metadata(file).unwrap().permissions().mode() & 0o777, 0o600);
        }
    }

    /// A new key would unpair every device, so a kept one that can't be read is an error, and it's left alone.
    #[test]
    fn what_cant_be_read_is_an_error_rather_than_a_reason_for_a_new_key() {
        let dir = tempfile::tempdir().unwrap();
        let (certificate, key) = (dir.path().join("hub-cert.der"), dir.path().join("hub-key.der"));
        load_or_create(&certificate, &key).unwrap();
        let kept_key = std::fs::read(&key).unwrap();

        std::fs::write(&certificate, b"not a certificate").unwrap();
        let unusable = load_or_create(&certificate, &key).map(|_| ());
        std::fs::remove_file(&key).unwrap();
        // Like a key without permission to read it, it can be replaced but not read. Root can read that, as the Linux
        // checks run, so this is a link to itself.
        std::os::unix::fs::symlink(&key, &key).unwrap();
        let unreadable = load_or_create(&certificate, &key).map(|_| ());

        assert!(matches!(unusable, Err(CertificateError::Kept { .. })), "{unusable:?}");
        assert!(matches!(unreadable, Err(CertificateError::Io { .. })), "{unreadable:?}");
        assert!(std::fs::symlink_metadata(&key).unwrap().is_symlink(), "the key was replaced");
        std::fs::remove_file(&key).unwrap();
        std::fs::write(&key, &kept_key).unwrap();
        assert!(load_or_create(&certificate, &key).is_err(), "the certificate wasn't replaced either");
    }

    #[test]
    fn the_file_that_cant_be_used_is_the_one_named() {
        let dir = tempfile::tempdir().unwrap();
        let (certificate, key) = (dir.path().join("hub-cert.der"), dir.path().join("hub-key.der"));
        load_or_create(&certificate, &key).unwrap();
        let named = |path: &Path| {
            let error = load_or_create(&certificate, &key).map(|_| ()).unwrap_err();
            matches!(&error, CertificateError::Kept { path: named, .. } if named == path)
        };

        let kept_key = std::fs::read(&key).unwrap();
        std::fs::write(&key, b"not a key").unwrap();
        assert!(named(&key), "a key that can't be used");
        std::fs::write(&key, kept_key).unwrap();
        std::fs::write(&certificate, b"not a certificate").unwrap();
        assert!(named(&certificate), "a certificate that can't be used");
    }

    #[test]
    fn a_missing_certificate_is_made_again_for_the_same_key() {
        let dir = tempfile::tempdir().unwrap();
        let (certificate, key) = (dir.path().join("hub-cert.der"), dir.path().join("hub-key.der"));
        let made = load_or_create(&certificate, &key).unwrap();

        std::fs::remove_file(&certificate).unwrap();
        let again = load_or_create(&certificate, &key).unwrap();

        assert_eq!(made.fingerprint, again.fingerprint);
        assert_eq!(std::fs::metadata(&certificate).unwrap().permissions().mode() & 0o777, 0o600);
    }

    #[test]
    fn the_fingerprint_is_of_the_key_rcgen_certified() {
        let key = rcgen::KeyPair::generate().unwrap();

        let fingerprint = fingerprint(&CertificateDer::from(certify(&key).unwrap())).unwrap();

        let expected =
            ring::digest::digest(&ring::digest::SHA256, &rcgen::PublicKeyData::subject_public_key_info(&key));
        assert_eq!(fingerprint, URL_SAFE_NO_PAD.encode(expected));
    }
}
