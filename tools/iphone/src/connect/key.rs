//! An App Store Connect key, and the token each request carries, which the key signs.

use std::path::{Path, PathBuf};

use anyhow::{Context as _, bail};
use base64::Engine as _;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use ring::rand::SystemRandom;
use ring::signature::{ECDSA_P256_SHA256_FIXED_SIGNING, EcdsaKeyPair};

/// How long a token is good for. App Store Connect takes up to 20 minutes, and each request makes a new one.
const TOKEN_LASTS_S: i64 = 10 * 60;

/// The key's ID, which App Store Connect lists beside it.
pub(crate) struct KeyId(String);

impl KeyId {
    pub(crate) fn new(id: String) -> Self {
        Self(id)
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

/// Who issued the key: the team's ID in App Store Connect, which it lists above its keys.
pub(crate) struct IssuerId(String);

impl IssuerId {
    pub(crate) fn new(id: String) -> Self {
        Self(id)
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

pub(crate) struct Key {
    path: PathBuf,
    id: KeyId,
    issuer: IssuerId,
    pair: EcdsaKeyPair,
}

impl Key {
    /// The key in the .p8 file at `path`, which Xcode reads too.
    pub(crate) fn read(path: PathBuf, id: KeyId, issuer: IssuerId) -> anyhow::Result<Self> {
        let pem = std::fs::read_to_string(&path).with_context(|| format!("couldn't read {}", path.display()))?;
        let pair = pair(&pem).with_context(|| format!("{} isn't an App Store Connect key", path.display()))?;
        Ok(Self { path, id, issuer, pair })
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    pub(crate) fn id(&self) -> &KeyId {
        &self.id
    }

    pub(crate) fn issuer(&self) -> &IssuerId {
        &self.issuer
    }

    /// A token for App Store Connect's API, from `now_s`, in seconds since 1970.
    pub(crate) fn token(&self, now_s: i64) -> anyhow::Result<String> {
        let header = serde_json::json!({ "alg": "ES256", "kid": self.id.0, "typ": "JWT" });
        let claims = serde_json::json!({
            "iss": self.issuer.0, "iat": now_s, "exp": now_s + TOKEN_LASTS_S, "aud": "appstoreconnect-v1",
        });
        let signed =
            format!("{}.{}", URL_SAFE_NO_PAD.encode(header.to_string()), URL_SAFE_NO_PAD.encode(claims.to_string()));
        // The fixed form, r then s, is the one a JWT carries.
        let signature =
            self.pair.sign(&SystemRandom::new(), signed.as_bytes()).map_err(|_| anyhow::anyhow!("couldn't sign"))?;
        Ok(format!("{signed}.{}", URL_SAFE_NO_PAD.encode(signature.as_ref())))
    }
}

/// The key pair in a .p8 file, a P-256 private key as PKCS#8 in PEM.
fn pair(pem: &str) -> anyhow::Result<EcdsaKeyPair> {
    let body: String = pem.lines().filter(|line| !line.starts_with("-----")).collect();
    if !pem.contains("-----BEGIN PRIVATE KEY-----") {
        bail!("it has no private key");
    }
    let der = STANDARD.decode(body.trim()).context("its key isn't base64")?;
    EcdsaKeyPair::from_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, &der, &SystemRandom::new())
        .map_err(|e| anyhow::anyhow!("its key isn't a P-256 key: {e}"))
}

#[cfg(test)]
mod tests {
    use ring::signature::{ECDSA_P256_SHA256_FIXED, KeyPair as _, UnparsedPublicKey};

    use super::*;

    #[test]
    fn a_token_says_who_and_when_and_is_signed_with_the_key() {
        let pkcs8 = EcdsaKeyPair::generate_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, &SystemRandom::new()).unwrap();
        let pem =
            format!("-----BEGIN PRIVATE KEY-----\n{}\n-----END PRIVATE KEY-----\n", STANDARD.encode(pkcs8.as_ref()));
        let file = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(file.path(), pem).unwrap();
        let key = Key::read(file.path().to_path_buf(), KeyId::new("ABC123".into()), IssuerId::new("issuer-1".into()))
            .unwrap();

        let token = key.token(1_000).unwrap();

        let parts: Vec<&str> = token.split('.').collect();
        let json = |part: &str| -> serde_json::Value {
            serde_json::from_slice(&URL_SAFE_NO_PAD.decode(part).unwrap()).unwrap()
        };
        assert_eq!(json(parts[0]), serde_json::json!({ "alg": "ES256", "kid": "ABC123", "typ": "JWT" }));
        assert_eq!(
            json(parts[1]),
            serde_json::json!({ "iss": "issuer-1", "iat": 1_000, "exp": 1_600, "aud": "appstoreconnect-v1" })
        );
        let public = UnparsedPublicKey::new(&ECDSA_P256_SHA256_FIXED, key.pair.public_key().as_ref().to_vec());
        let signed = format!("{}.{}", parts[0], parts[1]);
        assert!(public.verify(signed.as_bytes(), &URL_SAFE_NO_PAD.decode(parts[2]).unwrap()).is_ok());
    }
}
