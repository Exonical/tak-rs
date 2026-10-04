//! Plain TCP transport.

use async_trait::async_trait;
use tak_core::TransportId;
use tokio::net::{TcpStream, ToSocketAddrs};
use tokio::time::timeout;

use crate::error::NetworkError;
use crate::stream::StreamTransport;
use crate::transport::{Connector, Transport, TransportConfig};

/// Connects to a TAK peer over unencrypted TCP (TAK Server port 8087, ATAK
/// `tcp`/`stcp` mesh endpoints).
#[derive(Clone, Debug)]
pub struct TcpConnector {
    addr: String,
    config: TransportConfig,
}

impl TcpConnector {
    /// Target as `host:port`.
    pub fn new(addr: impl Into<String>, config: TransportConfig) -> Self {
        Self {
            addr: addr.into(),
            config,
        }
    }

    /// The configured target.
    pub fn addr(&self) -> &str {
        &self.addr
    }

    /// Connect once, applying `config.connect_timeout`.
    pub async fn connect_tcp(&self) -> Result<StreamTransport<TcpStream>, NetworkError> {
        connect(&self.addr, &self.config).await
    }
}

/// Connect to `addr` and wrap the socket.
pub async fn connect<A: ToSocketAddrs + Send>(
    addr: A,
    config: &TransportConfig,
) -> Result<StreamTransport<TcpStream>, NetworkError> {
    let stream = timeout(config.connect_timeout, TcpStream::connect(addr))
        .await
        .map_err(|_| NetworkError::Timeout(config.connect_timeout))??;
    stream.set_nodelay(true)?;
    Ok(wrap(stream, config.clone()))
}

/// Wrap an accepted or connected socket (useful for servers and tests).
pub fn wrap(stream: TcpStream, config: TransportConfig) -> StreamTransport<TcpStream> {
    let id = match stream.peer_addr() {
        Ok(peer) => TransportId::new(format!("tcp://{peer}")),
        Err(_) => TransportId::new("tcp://?"),
    };
    StreamTransport::new(stream, id, config)
}

#[async_trait]
impl Connector for TcpConnector {
    fn describe(&self) -> String {
        format!("tcp://{}", self.addr)
    }

    async fn connect(&self) -> Result<Box<dyn Transport>, NetworkError> {
        let t = self.connect_tcp().await?;
        tracing::debug!(target = %self.describe(), "TCP connected");
        Ok(Box::new(t))
    }
}
