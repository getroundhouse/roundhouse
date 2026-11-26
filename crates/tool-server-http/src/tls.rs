use std::error::Error;
use std::sync::Arc;

use hyper_rustls::{ConfigBuilderExt as _, HttpsConnectorBuilder};
use hyper_util::client::legacy::connect::HttpConnector;
use rustls::ClientConfig;
use rustls::crypto::ring;
use tokio::sync::OnceCell;

use crate::connecting::{Connector, ConnectsInTime, configured};

/// TLS on ring, trusting the system's certificates, which are read the first time a call needs them, since that takes
/// a moment and may never be needed.
#[derive(Default)]
pub struct Tls(OnceCell<ClientConfig>);

impl Tls {
    /// What connects to any server, finding it by the system's lookup of its name.
    pub async fn connector(&self) -> Result<Connector, Box<dyn Error + Send + Sync>> {
        let http = configured(HttpConnector::new());
        let https = HttpsConnectorBuilder::new().with_tls_config(self.config().await?).https_or_http().enable_http1();
        Ok(ConnectsInTime(https.wrap_connector(http)))
    }

    pub(crate) async fn config(&self) -> Result<ClientConfig, Box<dyn Error + Send + Sync>> {
        let read = || async {
            tokio::task::spawn_blocking(|| {
                let builder = ClientConfig::builder_with_provider(Arc::new(ring::default_provider()));
                let config = builder.with_safe_default_protocol_versions()?.with_native_roots()?;
                Ok::<_, Box<dyn Error + Send + Sync>>(config.with_no_client_auth())
            })
            .await?
        };
        Ok(self.0.get_or_try_init(read).await?.clone())
    }
}
