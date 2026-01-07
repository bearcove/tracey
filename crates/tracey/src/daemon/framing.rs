//! COBS framing for Unix socket streams.
//!
//! r[impl daemon.roam.framing]
//!
//! This is a copy of roam-tcp's CobsFramed adapted for UnixStream.

use std::io;

use cobs::{decode_vec as cobs_decode_vec, encode_vec as cobs_encode_vec};
use roam::__private::facet_postcard;
use roam_wire::Message;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;

/// A COBS-framed Unix socket connection.
///
/// Handles encoding/decoding of roam messages over a Unix stream using
/// COBS (Consistent Overhead Byte Stuffing) framing with 0x00 delimiters.
pub struct CobsFramedUnix {
    stream: UnixStream,
    buf: Vec<u8>,
    /// Last successfully decoded frame bytes (for error recovery/debugging).
    pub last_decoded: Vec<u8>,
}

impl CobsFramedUnix {
    /// Create a new framed connection from a Unix stream.
    pub fn new(stream: UnixStream) -> Self {
        Self {
            stream,
            buf: Vec::new(),
            last_decoded: Vec::new(),
        }
    }

    /// Send a message over the connection.
    pub async fn send(&mut self, msg: &Message) -> io::Result<()> {
        let payload = facet_postcard::to_vec(msg)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
        let mut framed = cobs_encode_vec(&payload);
        framed.push(0x00);
        self.stream.write_all(&framed).await?;
        self.stream.flush().await?;
        Ok(())
    }

    /// Receive a message with a timeout.
    ///
    /// Returns `Ok(None)` if no message received within timeout or connection closed.
    pub async fn recv_timeout(
        &mut self,
        timeout: std::time::Duration,
    ) -> io::Result<Option<Message>> {
        tokio::time::timeout(timeout, self.recv_inner())
            .await
            .unwrap_or(Ok(None))
    }

    /// Receive a message (blocking until one arrives or connection closes).
    #[allow(dead_code)]
    pub async fn recv(&mut self) -> io::Result<Option<Message>> {
        self.recv_inner().await
    }

    async fn recv_inner(&mut self) -> io::Result<Option<Message>> {
        loop {
            // Look for frame delimiter
            if let Some(idx) = self.buf.iter().position(|b| *b == 0x00) {
                let frame = self.buf.drain(..idx).collect::<Vec<_>>();
                self.buf.drain(..1); // Remove delimiter

                let decoded = cobs_decode_vec(&frame).map_err(|e| {
                    io::Error::new(io::ErrorKind::InvalidData, format!("cobs: {e}"))
                })?;
                self.last_decoded = decoded.clone();

                let msg: Message = facet_postcard::from_slice(&decoded).map_err(|e| {
                    io::Error::new(io::ErrorKind::InvalidData, format!("postcard: {e}"))
                })?;
                return Ok(Some(msg));
            }

            // Read more data
            let mut tmp = [0u8; 4096];
            let n = self.stream.read(&mut tmp).await?;
            if n == 0 {
                return Ok(None);
            }
            self.buf.extend_from_slice(&tmp[..n]);
        }
    }
}
