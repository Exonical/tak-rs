//! [`StreamTransport`]: a [`Transport`] over any byte stream.

use async_trait::async_trait;
use bytes::BufMut;
use tak_core::TransportId;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::time::timeout;

use crate::error::NetworkError;
use crate::frame::{Frame, StreamDecoder, StreamEncoder};
use crate::transport::{Transport, TransportConfig};

/// Frames TAK traffic over an `AsyncRead + AsyncWrite` stream.
///
/// Used directly for TCP and wrapped around TLS streams by `tak-transport`.
pub struct StreamTransport<S> {
    io: S,
    id: TransportId,
    config: TransportConfig,
    decoder: StreamDecoder,
    encoder: StreamEncoder,
    eof: bool,
}

impl<S> StreamTransport<S>
where
    S: AsyncRead + AsyncWrite + Unpin + Send,
{
    /// Wrap an already-connected stream.
    pub fn new(io: S, id: TransportId, config: TransportConfig) -> Self {
        Self {
            io,
            id,
            decoder: StreamDecoder::new(config.max_frame_bytes),
            encoder: StreamEncoder,
            config,
            eof: false,
        }
    }

    /// Borrow the underlying stream (e.g. to inspect TLS session state).
    pub fn get_ref(&self) -> &S {
        &self.io
    }

    /// Consume the transport and return the stream.
    pub fn into_inner(self) -> S {
        self.io
    }

    async fn fill(&mut self) -> Result<bool, NetworkError> {
        if self.eof {
            return Ok(false);
        }
        // Cap each read so a hostile peer cannot force huge allocations before
        // the frame-size check runs.
        let before = self.decoder.pending();
        let budget = self
            .config
            .max_frame_bytes
            .saturating_add(16)
            .saturating_sub(before)
            .clamp(1, 64 * 1024);
        let buf = self.decoder.buffer_mut();
        buf.reserve(budget.min(64 * 1024));
        let read = self.io.read_buf(&mut buf.limit(budget)).await?;
        if read == 0 {
            self.eof = true;
            return Ok(false);
        }
        Ok(true)
    }
}

#[async_trait]
impl<S> Transport for StreamTransport<S>
where
    S: AsyncRead + AsyncWrite + Unpin + Send,
{
    fn id(&self) -> &TransportId {
        &self.id
    }

    async fn send(&mut self, frame: Frame) -> Result<(), NetworkError> {
        if frame.len() > self.config.max_frame_bytes {
            return Err(NetworkError::FrameTooLarge {
                max: self.config.max_frame_bytes,
                actual: frame.len(),
            });
        }
        let bytes = self.encoder.encode(&frame);
        let write = async {
            self.io.write_all(&bytes).await?;
            self.io.flush().await
        };
        timeout(self.config.write_timeout, write)
            .await
            .map_err(|_| NetworkError::Timeout(self.config.write_timeout))??;
        tracing::trace!(transport = %self.id, bytes = bytes.len(), kind = ?frame.kind(), "sent frame");
        Ok(())
    }

    async fn recv(&mut self) -> Result<Option<Frame>, NetworkError> {
        loop {
            if let Some(frame) = self.decoder.next_frame()? {
                tracing::trace!(transport = %self.id, bytes = frame.len(), kind = ?frame.kind(), "received frame");
                return Ok(Some(frame));
            }
            let progressed = match self.config.read_idle_timeout {
                Some(idle) => timeout(idle, self.fill())
                    .await
                    .map_err(|_| NetworkError::Timeout(idle))??,
                None => self.fill().await?,
            };
            if !progressed {
                if self.decoder.pending() > 0 {
                    return Err(NetworkError::InvalidFrame(format!(
                        "peer closed with {} unframed bytes pending",
                        self.decoder.pending()
                    )));
                }
                return Ok(None);
            }
        }
    }

    async fn close(&mut self) -> Result<(), NetworkError> {
        self.eof = true;
        match timeout(self.config.write_timeout, self.io.shutdown()).await {
            Ok(Ok(())) => Ok(()),
            // Peer already went away; that is still "closed".
            Ok(Err(e)) if e.kind() == std::io::ErrorKind::NotConnected => Ok(()),
            Ok(Err(e)) => Err(e.into()),
            Err(_) => Err(NetworkError::Timeout(self.config.write_timeout)),
        }
    }
}
