//! The computer name and the IPv4 addresses Windows assigns to its active adapters.

use std::net::Ipv4Addr;
use windows_sys::Win32::Foundation::{ERROR_BUFFER_OVERFLOW, NO_ERROR};
use windows_sys::Win32::NetworkManagement::IpHelper::{GetAdaptersAddresses, IP_ADAPTER_ADDRESSES_LH};
use windows_sys::Win32::Networking::WinSock::{AF_INET, SOCKADDR_IN};
use windows_sys::Win32::System::WindowsProgramming::GetComputerNameW;

pub(crate) struct Machine {
    pub(crate) name: String,
    pub(crate) hosts: Vec<String>,
}

pub(crate) fn this_machine() -> Machine {
    let mut name = [0u16; 256];
    let mut length = 256;
    // SAFETY: the writable buffer has the length passed to Windows and lives through the call.
    let found = unsafe { GetComputerNameW(name.as_mut_ptr(), &raw mut length) };
    let name = if found == 0 { "PC".to_string() } else { String::from_utf16_lossy(&name[..length as usize]) };
    let mut hosts = ipv4_addresses().into_iter().map(|ip| ip.to_string()).collect::<Vec<_>>();
    if !hosts.contains(&name) {
        hosts.push(name.clone());
    }
    Machine { name, hosts }
}

fn ipv4_addresses() -> Vec<Ipv4Addr> {
    let mut bytes = 0;
    // SAFETY: this first call only writes the required size; no buffer is supplied.
    let result =
        unsafe { GetAdaptersAddresses(u32::from(AF_INET), 0, std::ptr::null(), std::ptr::null_mut(), &raw mut bytes) };
    if result != ERROR_BUFFER_OVERFLOW {
        return Vec::new();
    }
    // u64 storage provides the alignment required by IP_ADAPTER_ADDRESSES_LH on both Windows architectures.
    let mut buffer = vec![0u64; (bytes as usize).div_ceil(8)];
    let first = buffer.as_mut_ptr().cast::<IP_ADAPTER_ADDRESSES_LH>();
    // SAFETY: first points to aligned writable storage of at least bytes bytes, kept for the traversal below.
    if unsafe { GetAdaptersAddresses(u32::from(AF_INET), 0, std::ptr::null(), first, &raw mut bytes) } != NO_ERROR {
        return Vec::new();
    }
    let mut addresses = Vec::new();
    let mut next = first;
    while !next.is_null() {
        // SAFETY: Windows returned this linked list inside buffer, which is still alive.
        let adapter = unsafe { &*next };
        let mut unicast = adapter.FirstUnicastAddress;
        while adapter.OperStatus == 1 && !unicast.is_null() {
            // SAFETY: each address is in the list returned by GetAdaptersAddresses for AF_INET.
            let entry = unsafe { &*unicast };
            if !entry.Address.lpSockaddr.is_null() {
                // SAFETY: AF_INET was requested; the OS supplied a SOCKADDR_IN at this address.
                let address = unsafe { entry.Address.lpSockaddr.cast::<SOCKADDR_IN>().read_unaligned() };
                // SAFETY: S_addr is the IPv4 address representation of this IN_ADDR union.
                let ip = Ipv4Addr::from(unsafe { address.sin_addr.S_un.S_addr }.to_ne_bytes());
                if !ip.is_loopback() && !ip.is_link_local() && !addresses.contains(&ip) {
                    addresses.push(ip);
                }
            }
            unicast = entry.Next;
        }
        next = adapter.Next;
    }
    addresses
}
