use std::io;
use std::net::{IpAddr, SocketAddr};

use http::Uri;
use http::uri::Scheme;

/// Where a name's addresses come from.
pub enum Names {
    /// The system, as for any program.
    System,
    /// Made up for a test, which can't otherwise have a name come to an address it chooses, such as one on the
    /// internet, without connecting to it.
    #[cfg(any(test, feature = "testing"))]
    Fixed(std::collections::HashMap<&'static str, Vec<IpAddr>>),
}

impl Names {
    /// Every address `uri`'s host comes to, with its port, or its scheme's.
    pub async fn at(&self, uri: &Uri) -> io::Result<Vec<SocketAddr>> {
        let host = uri.host().ok_or_else(|| io::Error::other("an address with no host"))?;
        let own_port = if uri.scheme() == Some(&Scheme::HTTP) { 80 } else { 443 };
        self.of(host, uri.port_u16().unwrap_or(own_port)).await
    }

    /// Every address `host` comes to, with `port`. A host that's an address, as a URL has it, with an IPv6 one in
    /// brackets, comes to that address alone, and isn't looked up.
    pub async fn of(&self, host: &str, port: u16) -> io::Result<Vec<SocketAddr>> {
        let bare = host.strip_prefix('[').and_then(|host| host.strip_suffix(']')).unwrap_or(host);
        if let Ok(address) = bare.parse::<IpAddr>() {
            return Ok(vec![(address, port).into()]);
        }
        match self {
            Names::System => Ok(tokio::net::lookup_host((host, port)).await?.collect()),
            #[cfg(any(test, feature = "testing"))]
            Names::Fixed(names) => {
                let found = names.get(host).ok_or_else(|| io::Error::other("no such name"))?;
                Ok(found.iter().map(|address| (*address, port).into()).collect())
            }
        }
    }
}
