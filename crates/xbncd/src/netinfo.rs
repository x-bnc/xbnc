//! Local network helpers: which addresses count as "local network", and
//! which address to show users when a service listens on 0.0.0.0 / [::].

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, UdpSocket};

/// True for addresses that can only come from this machine or the local
/// network: loopback, RFC 1918, link-local, CGNAT (100.64.0.0/10, used by
/// Tailscale and some home routers) and IPv6 unique-local addresses.
pub fn is_local_network(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_local_v4(v4),
        IpAddr::V6(v6) => {
            if let Some(v4) = v6.to_ipv4_mapped() {
                return is_local_v4(v4);
            }
            let seg0 = v6.segments()[0];
            v6.is_loopback()
                || (seg0 & 0xfe00) == 0xfc00 // fc00::/7 unique local
                || (seg0 & 0xffc0) == 0xfe80 // fe80::/10 link local
        }
    }
}

fn is_local_v4(v4: Ipv4Addr) -> bool {
    let o = v4.octets();
    v4.is_loopback()
        || v4.is_private()
        || v4.is_link_local()
        || (o[0] == 100 && (o[1] & 0xc0) == 64)
}

/// The address this machine uses for outgoing traffic, found without sending
/// any packet (connecting a UDP socket only selects a route).
pub fn primary_ip(v6: bool) -> Option<IpAddr> {
    let (bind, probe): (SocketAddr, SocketAddr) = if v6 {
        (
            (Ipv6Addr::UNSPECIFIED, 0).into(),
            "[2001:db8::1]:9".parse().ok()?,
        )
    } else {
        (
            (Ipv4Addr::UNSPECIFIED, 0).into(),
            "192.0.2.1:9".parse().ok()?,
        )
    };
    let s = UdpSocket::bind(bind).ok()?;
    s.connect(probe).ok()?;
    let ip = s.local_addr().ok()?.ip();
    (!ip.is_unspecified() && !ip.is_loopback()).then_some(ip)
}

/// Host to show users for a listener: the listener address itself, or the
/// machine's primary address when it listens on every interface.
pub fn display_host(addr: SocketAddr) -> String {
    let ip = if addr.ip().is_unspecified() {
        primary_ip(addr.is_ipv6() && primary_ip(false).is_none())
            .unwrap_or(IpAddr::V4(Ipv4Addr::LOCALHOST))
    } else {
        addr.ip()
    };
    match ip {
        IpAddr::V6(v6) => format!("[{v6}]"),
        IpAddr::V4(v4) => v4.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_network_ranges() {
        for ok in [
            "127.0.0.1",
            "10.1.2.3",
            "172.16.0.1",
            "172.31.255.255",
            "192.168.1.139",
            "169.254.1.1",
            "100.64.0.1",
            "100.127.255.255",
            "::1",
            "fd00::1",
            "fe80::1",
            "::ffff:192.168.1.5",
        ] {
            assert!(is_local_network(ok.parse().unwrap()), "{ok}");
        }
        for bad in [
            "8.8.8.8",
            "172.32.0.1",
            "100.128.0.1",
            "2001:4860::8888",
            "::ffff:8.8.8.8",
        ] {
            assert!(!is_local_network(bad.parse().unwrap()), "{bad}");
        }
    }

    #[test]
    fn display_host_keeps_explicit_addresses() {
        assert_eq!(
            display_host("192.168.1.5:6697".parse().unwrap()),
            "192.168.1.5"
        );
        assert_eq!(display_host("[::1]:6697".parse().unwrap()), "[::1]");
        assert!(!display_host("0.0.0.0:6697".parse().unwrap()).contains("0.0.0.0"));
    }
}
