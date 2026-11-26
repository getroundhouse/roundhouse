use std::io;
use std::net::SocketAddr;

use url::{Host, Url};

/// Where a name's addresses come from.
#[derive(Clone)]
pub(crate) enum Names {
    /// The system, as for any program.
    System,
    /// Made up for a test.
    #[cfg(test)]
    Fixed(std::collections::HashMap<&'static str, Vec<std::net::IpAddr>>),
}

impl Names {
    /// Every address `url`'s host comes to, or its own where it is one, with `url`'s port.
    pub(super) async fn look_up(&self, url: &Url) -> io::Result<Vec<SocketAddr>> {
        let port = url.port_or_known_default().unwrap_or_default();
        let name = match url.host() {
            Some(Host::Ipv4(address)) => return Ok(vec![(address, port).into()]),
            Some(Host::Ipv6(address)) => return Ok(vec![(address, port).into()]),
            Some(Host::Domain(name)) => name,
            None => return Err(io::Error::other("an address with no host")),
        };
        self.of(name, port).await
    }

    /// Every address `host` comes to, which can be one, with `port`.
    pub(crate) async fn of(&self, host: &str, port: u16) -> io::Result<Vec<SocketAddr>> {
        match self {
            Names::System => Ok(tokio::net::lookup_host((host, port)).await?.collect()),
            #[cfg(test)]
            Names::Fixed(names) => {
                let found = names.get(host).ok_or_else(|| io::Error::other("no such name"))?;
                Ok(found.iter().map(|address| (*address, port).into()).collect())
            }
        }
    }
}
