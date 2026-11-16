//! What a phone needs to find this machine: what it's called, and the hosts it might reach it at. Read each time a
//! pairing link is made, because a laptop's addresses change as it moves.

use std::net::Ipv4Addr;
use std::process::Command;

pub(crate) struct Machine {
    /// For people, such as "Ada's `MacBook` Pro".
    pub(crate) name: String,
    /// The machine's `.local` name, then each IPv4 address that isn't loopback or link-local, which includes a
    /// Tailscale address. Without ports.
    pub(crate) hosts: Vec<String>,
}

/// Blocks, since on a Mac it asks `scutil`.
pub(crate) fn this_machine() -> Machine {
    let host_name = host_name();
    let first_label = host_name.split('.').next().unwrap_or_default().to_string();
    let name = scutil("ComputerName").unwrap_or_else(|| first_label.clone());
    // On a Mac, the name Bonjour answers to, which a DHCP server can't change the way it can the host name.
    let local = scutil("LocalHostName").unwrap_or(first_label);
    let mut hosts: Vec<String> = Vec::new();
    if !local.is_empty() {
        hosts.push(format!("{local}.local"));
    }
    for address in ipv4_addresses() {
        if !address.is_loopback() && !address.is_link_local() && !hosts.contains(&address.to_string()) {
            hosts.push(address.to_string());
        }
    }
    Machine { name, hosts }
}

/// One of the machine's names from `scutil --get`, on a Mac.
fn scutil(name: &str) -> Option<String> {
    if !cfg!(target_os = "macos") {
        return None;
    }
    let output = Command::new("scutil").args(["--get", name]).output().ok()?;
    let value = String::from_utf8(output.stdout).ok()?.trim().to_string();
    (output.status.success() && !value.is_empty()).then_some(value)
}

fn host_name() -> String {
    let mut buffer = [0u8; 256];
    // SAFETY: gethostname writes at most the length it's given into the buffer, which outlives the call.
    let result = unsafe { libc::gethostname(buffer.as_mut_ptr().cast(), buffer.len()) };
    if result != 0 {
        return String::new();
    }
    let end = buffer.iter().position(|&b| b == 0).unwrap_or(buffer.len());
    String::from_utf8_lossy(&buffer[..end]).into_owned()
}

/// Every IPv4 address on an interface that's up, in the order the OS lists them.
fn ipv4_addresses() -> Vec<Ipv4Addr> {
    let mut list: *mut libc::ifaddrs = std::ptr::null_mut();
    // SAFETY: getifaddrs only writes the pointer it's given, to a list it allocates, which is freed below.
    if unsafe { libc::getifaddrs(&raw mut list) } != 0 {
        return Vec::new();
    }
    let mut addresses = Vec::new();
    let mut entry = list;
    while !entry.is_null() {
        // SAFETY: each entry is in the list getifaddrs made, which stays allocated until freeifaddrs.
        let interface = unsafe { &*entry };
        if interface.ifa_flags & libc::IFF_UP.cast_unsigned() != 0
            && let Some(address) = ipv4_of(interface.ifa_addr)
        {
            addresses.push(address);
        }
        entry = interface.ifa_next;
    }
    // SAFETY: the list came from getifaddrs, and nothing borrowed from it is used after this.
    unsafe { libc::freeifaddrs(list) };
    addresses
}

fn ipv4_of(address: *const libc::sockaddr) -> Option<Ipv4Addr> {
    if address.is_null() {
        return None;
    }
    // SAFETY: a non-null interface address points to a sockaddr, whose family is in the same place in every kind.
    let family = unsafe { (*address).sa_family };
    if i32::from(family) != libc::AF_INET {
        return None;
    }
    // SAFETY: an AF_INET address is a sockaddr_in. Read unaligned, since a sockaddr only promises byte alignment.
    let ipv4 = unsafe { std::ptr::read_unaligned(address.cast::<libc::sockaddr_in>()) };
    Some(Ipv4Addr::from(u32::from_be(ipv4.sin_addr.s_addr)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_machine_offers_its_local_name_first_and_no_loopback_address() {
        let machine = this_machine();

        assert!(!machine.name.is_empty());
        assert!(
            machine.hosts.first().is_some_and(|host| host.to_ascii_lowercase().ends_with(".local")),
            "{:?}",
            machine.hosts
        );
        assert!(!machine.hosts.iter().any(|host| host.starts_with("127.")), "{:?}", machine.hosts);
    }
}
