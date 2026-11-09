//! Trusting a hub by the key it was paired with, rather than by a certificate authority or a name.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use protocol::pairing::Fingerprint;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{CryptoProvider, verify_tls12_signature, verify_tls13_signature};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{CertificateError, DigitallySignedStruct, SignatureScheme};

/// Accepts only a certificate for the pinned key. Its names, dates and issuer don't matter, since the hub signed it
/// itself, and the handshake's signature still proves the hub holds the key.
#[derive(Debug)]
pub(super) struct PinnedKey {
    fingerprint: Fingerprint,
    provider: Arc<CryptoProvider>,
    /// Set once it has refused a certificate, so a failed handshake can say why.
    refused: AtomicBool,
}

impl PinnedKey {
    pub(super) fn new(fingerprint: &Fingerprint, provider: Arc<CryptoProvider>) -> PinnedKey {
        PinnedKey { fingerprint: fingerprint.clone(), provider, refused: AtomicBool::new(false) }
    }

    pub(super) fn refused(&self) -> bool {
        self.refused.load(Ordering::Relaxed)
    }
}

/// What a pairing link would carry for this certificate. The hub works it out from the same bytes, in
/// `hub::remote::certificate`, and pairing fails if the two ever differ.
fn fingerprint(certificate: &CertificateDer<'_>) -> Option<Fingerprint> {
    let key = webpki::EndEntityCert::try_from(certificate).ok()?.subject_public_key_info();
    let mut sha256 = [0; 32];
    sha256.copy_from_slice(ring::digest::digest(&ring::digest::SHA256, key.as_ref()).as_ref());
    Some(Fingerprint::of_sha256(sha256))
}

impl ServerCertVerifier for PinnedKey {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        if fingerprint(end_entity).is_some_and(|found| found == self.fingerprint) {
            return Ok(ServerCertVerified::assertion());
        }
        self.refused.store(true, Ordering::Relaxed);
        Err(rustls::Error::InvalidCertificate(CertificateError::ApplicationVerificationFailure))
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        certificate: &CertificateDer<'_>,
        signature: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls12_signature(message, certificate, signature, &self.provider.signature_verification_algorithms)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        certificate: &CertificateDer<'_>,
        signature: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls13_signature(message, certificate, signature, &self.provider.signature_verification_algorithms)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.provider.signature_verification_algorithms.supported_schemes()
    }
}
