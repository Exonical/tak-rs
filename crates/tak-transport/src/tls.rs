//! TLS transport (TAK Server port 8089 / `ssl` endpoints).

use std::sync::Arc;

use async_trait::async_trait;
use rustls::ClientConfig;
use rustls_pki_types::ServerName;
use tak_core::TransportId;
use tak_network::{Connector, NetworkError, StreamTransport, Transport, TransportConfig};
use tokio::net::TcpStream;
use tokio::time::timeout;
use tokio_rustls::client::TlsStream;

use crate::error::TransportError;

/// Connects with TLS using a `rustls` config from `tak-crypto`.
#[derive(Clone)]
pub struct TlsConnector {
    host: String,
    port: u16,
    server_name: ServerName<'static>,
    tls: Arc<ClientConfig>,
    config: TransportConfig,
}

impl std::fmt::Debug for TlsConnector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TlsConnector")
            .field("host", &self.host)
            .field("port", &self.port)
            .finish_non_exhaustive()
    }
}

impl TlsConnector {
    /// Target host (DNS name or IP literal) and port. The host is also used
    /// as the SNI/verification name.
    pub fn new(
        host: impl Into<String>,
        port: u16,
        tls: Arc<ClientConfig>,
        config: TransportConfig,
    ) -> Result<Self, TransportError> {
        let host = host.into();
        let server_name = tak_crypto::config::server_name(&host)?;
        Ok(Self {
            host,
            port,
            server_name,
            tls,
            config,
        })
    }

    /// Connect once (TCP + TLS handshake, both under `connect_timeout`).
    pub async fn connect_tls(&self) -> Result<StreamTransport<TlsStream<TcpStream>>, NetworkError> {
        let deadline = self.config.connect_timeout;
        let tcp = timeout(
            deadline,
            TcpStream::connect((self.host.as_str(), self.port)),
        )
        .await
        .map_err(|_| NetworkError::Timeout(deadline))??;
        tcp.set_nodelay(true)?;
        let connector = tokio_rustls::TlsConnector::from(self.tls.clone());
        let tls = timeout(deadline, connector.connect(self.server_name.clone(), tcp))
            .await
            .map_err(|_| NetworkError::Timeout(deadline))??;
        let id = match tls.get_ref().0.peer_addr() {
            Ok(peer) => TransportId::new(format!("ssl://{peer}")),
            Err(_) => TransportId::new(format!("ssl://{}:{}", self.host, self.port)),
        };
        Ok(StreamTransport::new(tls, id, self.config.clone()))
    }
}

#[async_trait]
impl Connector for TlsConnector {
    fn describe(&self) -> String {
        format!("ssl://{}:{}", self.host, self.port)
    }

    async fn connect(&self) -> Result<Box<dyn Transport>, NetworkError> {
        let t = self.connect_tls().await?;
        tracing::debug!(target = %self.describe(), "TLS connected");
        Ok(Box::new(t))
    }
}
