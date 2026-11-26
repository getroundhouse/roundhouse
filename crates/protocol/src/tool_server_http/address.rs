use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

/// The IPv4 networks that the IANA's registry of special-purpose addresses has, and multicast's, each with how many
/// bits name it.
const PRIVATE: [(Ipv4Addr, u32); 15] = [
    // This network.
    (Ipv4Addr::UNSPECIFIED, 8),
    // Private networks.
    (Ipv4Addr::new(10, 0, 0, 0), 8),
    (Ipv4Addr::new(172, 16, 0, 0), 12),
    (Ipv4Addr::new(192, 168, 0, 0), 16),
    // What a carrier's customers share, and a VPN's machines.
    (Ipv4Addr::new(100, 64, 0, 0), 10),
    // Loopback.
    (Ipv4Addr::new(127, 0, 0, 0), 8),
    // The link's, such as a cloud's metadata service.
    (Ipv4Addr::new(169, 254, 0, 0), 16),
    // For protocols.
    (Ipv4Addr::new(192, 0, 0, 0), 24),
    // For documentation.
    (Ipv4Addr::new(192, 0, 2, 0), 24),
    (Ipv4Addr::new(198, 51, 100, 0), 24),
    (Ipv4Addr::new(203, 0, 113, 0), 24),
    // 6to4's old relays.
    (Ipv4Addr::new(192, 88, 99, 0), 24),
    // For measuring.
    (Ipv4Addr::new(198, 18, 0, 0), 15),
    // Multicast.
    (Ipv4Addr::new(224, 0, 0, 0), 4),
    // Kept for later, with broadcast.
    (Ipv4Addr::new(240, 0, 0, 0), 4),
];

/// Whether `address` is anything but one machine's on the internet: this machine's, a private network's, the link's,
/// or kept for something else. A server's sign-in goes over plain `http` only to such an address, both as the hub signs
/// in and as a worker passes an agent's call on, since nobody on the internet between can read it there.
pub fn is_private(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(address) => is_private_v4(address),
        IpAddr::V6(address) => is_private_v6(address),
    }
}

fn is_private_v4(address: Ipv4Addr) -> bool {
    PRIVATE.iter().any(|(network, bits)| (address.to_bits() ^ network.to_bits()) >> (32 - bits) == 0)
}

/// An address that stands for an IPv4 one is what that one is.
fn is_private_v6(address: Ipv6Addr) -> bool {
    if let Some(mapped) = address.to_ipv4_mapped() {
        return is_private_v4(mapped);
    }
    match address.segments() {
        // Translated to IPv4 by NAT64, and 6to4.
        [0x64, 0xff9b, 0, 0, 0, 0, high, low] | [0x2002, high, low, ..] => {
            is_private_v4(Ipv4Addr::from_bits((u32::from(high) << 16) | u32::from(low)))
        }
        // For protocols, such as Teredo, and documentation.
        [0x2001, 0..=0x1ff | 0xdb8, ..] | [0x3fff, 0..=0xfff, ..] => true,
        // All that's given out to machines on the internet. Loopback, the link's, private and multicast are outside it.
        [first, ..] => !(0x2000..=0x3fff).contains(&first),
    }
}
