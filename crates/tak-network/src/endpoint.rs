//! TAK endpoint strings (`host:port:proto`).
//!
//! TAK clients describe connections as `192.168.1.10:8089:ssl`; contacts
//! advertise `*:-1:stcp` (meaning "reach me through the server I am
//! connected to") or `10.0.0.5:4242:tcp` for direct mesh connections.

use std::fmt;
use std::str::FromStr;

use crate::error::NetworkError;

/// Transport protocol named in an endpoint string.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Protocol {
    /// Plain TCP stream (`tcp`).
    Tcp,
    /// TLS over TCP (`ssl`), the normal TAK Server client port (8089).
    Tls,
    /// UDP datagrams (`udp`), multicast SA.
    Udp,
    /// "Streaming TCP" (`stcp`): the TAK Server relay endpoint advertised by
    /// contacts. Wire-identical to [`Protocol::Tcp`].
    Stcp,
    /// QUIC (`quic`), TAK Server 4.8+.
    Quic,
}

impl Protocol {
    /// Canonical lowercase name as used in endpoint strings.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Tcp => "tcp",
            Self::Tls => "ssl",
            Self::Udp => "udp",
            Self::Stcp => "stcp",
            Self::Quic => "quic",
        }
    }

    /// Whether this protocol carries a byte stream (vs. datagrams).
    pub fn is_stream(self) -> bool {
        matches!(self, Self::Tcp | Self::Tls | Self::Stcp | Self::Quic)
    }
}

impl FromStr for Protocol {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "tcp" => Ok(Self::Tcp),
            "ssl" | "tls" => Ok(Self::Tls),
            "udp" => Ok(Self::Udp),
            "stcp" => Ok(Self::Stcp),
            "quic" => Ok(Self::Quic),
            _ => Err(()),
        }
    }
}

impl fmt::Display for Protocol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A parsed `host:port:proto` endpoint.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Endpoint {
    /// Hostname, IPv4/IPv6 literal, or `*` for "via the server".
    pub host: String,
    /// Port, or `None` when the string carried `-1`.
    pub port: Option<u16>,
    /// Protocol.
    pub protocol: Protocol,
}

impl Endpoint {
    /// Build an endpoint.
    pub fn new(host: impl Into<String>, port: u16, protocol: Protocol) -> Self {
        Self {
            host: host.into(),
            port: Some(port),
            protocol,
        }
    }

    /// The `*:-1:stcp` relay endpoint that TAK Server-connected clients
    /// advertise in their `<contact>` detail.
    pub fn server_relay() -> Self {
        Self {
            host: "*".into(),
            port: None,
            protocol: Protocol::Stcp,
        }
    }

    /// Whether this endpoint can only be reached through a shared server.
    pub fn is_relay(&self) -> bool {
        self.host == "*" || self.port.is_none()
    }

    /// `host:port` suitable for `TcpStream::connect`, if directly reachable.
    pub fn socket_addr_string(&self) -> Option<String> {
        if self.is_relay() {
            return None;
        }
        let port = self.port?;
        if self.host.contains(':') && !self.host.starts_with('[') {
            Some(format!("[{}]:{port}", self.host))
        } else {
            Some(format!("{}:{port}", self.host))
        }
    }
}

impl FromStr for Endpoint {
    type Err = NetworkError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let err = |reason: &str| NetworkError::InvalidEndpoint {
            endpoint: s.to_owned(),
            reason: reason.to_owned(),
        };
        let s = s.trim();
        // Split from the right: the host may be an IPv6 literal with colons.
        let (rest, proto) = s
            .rsplit_once(':')
            .ok_or_else(|| err("expected host:port:proto"))?;
        let (host, port) = rest
            .rsplit_once(':')
            .ok_or_else(|| err("expected host:port:proto"))?;
        let host = host.trim_start_matches('[').trim_end_matches(']');
        if host.is_empty() {
            return Err(err("empty host"));
        }
        let protocol: Protocol = proto.parse().map_err(|()| err("unknown protocol"))?;
        let port = match port.trim() {
            "-1" => None,
            p => Some(
                p.parse::<u16>()
                    .map_err(|_| err("port must be 0-65535 or -1"))?,
            ),
        };
        Ok(Self {
            host: host.to_owned(),
            port,
            protocol,
        })
    }
}

impl fmt::Display for Endpoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let host = if self.host.contains(':') {
            format!("[{}]", self.host)
        } else {
            self.host.clone()
        };
        match self.port {
            Some(p) => write!(f, "{host}:{p}:{}", self.protocol),
            None => write!(f, "{host}:-1:{}", self.protocol),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_common_forms() {
        let e: Endpoint = "192.168.1.10:8089:ssl".parse().unwrap();
        assert_eq!(e, Endpoint::new("192.168.1.10", 8089, Protocol::Tls));
        assert_eq!(e.socket_addr_string().as_deref(), Some("192.168.1.10:8089"));
        assert_eq!(e.to_string(), "192.168.1.10:8089:ssl");

        let relay: Endpoint = "*:-1:stcp".parse().unwrap();
        assert_eq!(relay, Endpoint::server_relay());
        assert!(relay.is_relay());
        assert_eq!(relay.socket_addr_string(), None);
        assert_eq!(relay.to_string(), "*:-1:stcp");

        let v6: Endpoint = "[fe80::1]:4242:tcp".parse().unwrap();
        assert_eq!(v6.host, "fe80::1");
        assert_eq!(v6.socket_addr_string().as_deref(), Some("[fe80::1]:4242"));
        assert_eq!(v6.to_string(), "[fe80::1]:4242:tcp");

        let bare_v6: Endpoint = "fe80::1:4242:udp".parse().unwrap();
        assert_eq!(bare_v6.host, "fe80::1");
        assert_eq!(bare_v6.protocol, Protocol::Udp);
    }

    #[test]
    fn rejects_garbage() {
        for bad in [
            "",
            "host",
            "host:1",
            "host:99999:tcp",
            "host:1:carrierpigeon",
            ":1:tcp",
        ] {
            assert!(bad.parse::<Endpoint>().is_err(), "{bad}");
        }
    }
}
