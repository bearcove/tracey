//! roam connection handling over Unix sockets.
//!
//! This is adapted from roam-tcp's connection.rs for Unix socket transport.

use std::time::Duration;

use roam::session::{OutgoingPoll, Role, StreamError, StreamIdAllocator, StreamRegistry};
use roam_wire::{Hello, Message};
use tracey_proto::{TraceyDaemon, tracey_daemon_dispatch_unary};

use super::framing::CobsFramedUnix;

/// Negotiated connection parameters after Hello exchange.
#[derive(Debug, Clone)]
pub struct Negotiated {
    /// Effective max payload size (min of both peers).
    pub max_payload_size: u32,
    /// Initial stream credit (min of both peers).
    #[allow(dead_code)]
    pub initial_credit: u32,
}

/// Error during connection handling.
#[derive(Debug)]
pub enum ConnectionError {
    /// IO error.
    Io(std::io::Error),
    /// Protocol violation requiring Goodbye.
    ProtocolViolation {
        /// Rule ID that was violated.
        rule_id: &'static str,
        /// Human-readable context.
        #[allow(dead_code)]
        context: String,
    },
    /// Dispatch error.
    Dispatch(String),
    /// Connection closed cleanly.
    Closed,
}

impl From<std::io::Error> for ConnectionError {
    fn from(e: std::io::Error) -> Self {
        ConnectionError::Io(e)
    }
}

/// A live connection with completed Hello exchange.
pub struct Connection {
    io: CobsFramedUnix,
    #[allow(dead_code)]
    role: Role,
    negotiated: Negotiated,
    #[allow(dead_code)]
    stream_allocator: StreamIdAllocator,
    stream_registry: StreamRegistry,
    #[allow(dead_code)]
    our_hello: Hello,
}

impl Connection {
    /// Send a Goodbye message and return an error.
    pub async fn goodbye(&mut self, rule_id: &'static str) -> ConnectionError {
        let _ = self
            .io
            .send(&Message::Goodbye {
                reason: rule_id.into(),
            })
            .await;
        ConnectionError::ProtocolViolation {
            rule_id,
            context: String::new(),
        }
    }

    /// Validate payload size against negotiated limit.
    pub fn validate_payload_size(&self, size: usize) -> Result<(), &'static str> {
        if size as u32 > self.negotiated.max_payload_size {
            return Err("flow.unary.payload-limit");
        }
        Ok(())
    }

    /// Send all pending outgoing stream messages.
    pub async fn flush_outgoing(&mut self) -> Result<(), ConnectionError> {
        loop {
            match self.stream_registry.poll_outgoing() {
                OutgoingPoll::Data { stream_id, payload } => {
                    let msg = Message::Data { stream_id, payload };
                    self.io.send(&msg).await?;
                }
                OutgoingPoll::Close { stream_id } => {
                    let msg = Message::Close { stream_id };
                    self.io.send(&msg).await?;
                }
                OutgoingPoll::Pending | OutgoingPoll::Done => {
                    break;
                }
            }
        }
        Ok(())
    }

    /// Run the message loop with a TraceyDaemon service.
    pub async fn run<S: TraceyDaemon + ?Sized>(
        &mut self,
        service: &S,
    ) -> Result<(), ConnectionError> {
        loop {
            let msg = match self.io.recv_timeout(Duration::from_secs(30)).await {
                Ok(Some(m)) => m,
                Ok(None) => return Ok(()),
                Err(e) => {
                    let raw = &self.io.last_decoded;
                    if raw.len() >= 2 && raw[0] == 0x00 && raw[1] != 0x00 {
                        return Err(self.goodbye("message.hello.unknown-version").await);
                    }
                    return Err(ConnectionError::Io(e));
                }
            };

            match msg {
                Message::Hello(_) => {
                    // Duplicate Hello after exchange - ignore
                }
                Message::Goodbye { .. } => {
                    return Ok(());
                }
                Message::Request {
                    request_id,
                    method_id,
                    metadata: _,
                    payload,
                } => {
                    if let Err(rule_id) = self.validate_payload_size(payload.len()) {
                        return Err(self.goodbye(rule_id).await);
                    }

                    // Dispatch to service
                    let response_payload =
                        tracey_daemon_dispatch_unary(service, method_id, &payload)
                            .await
                            .map_err(|e| ConnectionError::Dispatch(format!("{:?}", e)))?;

                    let resp = Message::Response {
                        request_id,
                        metadata: Vec::new(),
                        payload: response_payload,
                    };
                    self.io.send(&resp).await?;

                    self.flush_outgoing().await?;
                }
                Message::Response { .. } | Message::Cancel { .. } => {
                    // Server doesn't expect these
                }
                Message::Data { stream_id, payload } => {
                    if stream_id == 0 {
                        return Err(self.goodbye("streaming.id.zero-reserved").await);
                    }

                    match self.stream_registry.route_data(stream_id, payload).await {
                        Ok(()) => {}
                        Err(StreamError::Unknown) => {
                            return Err(self.goodbye("streaming.unknown").await);
                        }
                        Err(StreamError::DataAfterClose) => {
                            return Err(self.goodbye("streaming.data-after-close").await);
                        }
                    }
                }
                Message::Close { stream_id } => {
                    if stream_id == 0 {
                        return Err(self.goodbye("streaming.id.zero-reserved").await);
                    }
                    if !self.stream_registry.contains(stream_id) {
                        return Err(self.goodbye("streaming.unknown").await);
                    }
                    self.stream_registry.close(stream_id);
                }
                Message::Reset { stream_id } => {
                    if stream_id == 0 {
                        return Err(self.goodbye("streaming.id.zero-reserved").await);
                    }
                    if !self.stream_registry.contains(stream_id) {
                        return Err(self.goodbye("streaming.unknown").await);
                    }
                    self.stream_registry.close(stream_id);
                }
                Message::Credit { stream_id, .. } => {
                    if stream_id == 0 {
                        return Err(self.goodbye("streaming.id.zero-reserved").await);
                    }
                    if !self.stream_registry.contains(stream_id) {
                        return Err(self.goodbye("streaming.unknown").await);
                    }
                }
            }
        }
    }
}

/// Perform Hello exchange as the acceptor (daemon side).
pub async fn hello_exchange_acceptor(
    mut io: CobsFramedUnix,
    our_hello: Hello,
) -> Result<Connection, ConnectionError> {
    // Send our Hello immediately
    io.send(&Message::Hello(our_hello.clone())).await?;

    // Wait for peer Hello
    let peer_hello = match io.recv_timeout(Duration::from_secs(5)).await? {
        Some(Message::Hello(h)) => h,
        Some(_) => {
            let _ = io
                .send(&Message::Goodbye {
                    reason: "message.hello.ordering".into(),
                })
                .await;
            return Err(ConnectionError::ProtocolViolation {
                rule_id: "message.hello.ordering",
                context: "received non-Hello before Hello exchange".into(),
            });
        }
        None => return Err(ConnectionError::Closed),
    };

    let (our_max, our_credit) = match &our_hello {
        Hello::V1 {
            max_payload_size,
            initial_stream_credit,
        } => (*max_payload_size, *initial_stream_credit),
    };
    let (peer_max, peer_credit) = match &peer_hello {
        Hello::V1 {
            max_payload_size,
            initial_stream_credit,
        } => (*max_payload_size, *initial_stream_credit),
    };

    let negotiated = Negotiated {
        max_payload_size: our_max.min(peer_max),
        initial_credit: our_credit.min(peer_credit),
    };

    Ok(Connection {
        io,
        role: Role::Acceptor,
        negotiated,
        stream_allocator: StreamIdAllocator::new(Role::Acceptor),
        stream_registry: StreamRegistry::new(),
        our_hello,
    })
}
