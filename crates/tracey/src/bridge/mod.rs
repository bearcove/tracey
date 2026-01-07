//! Protocol bridges to the tracey daemon.
//!
//! Each bridge translates a specific protocol (HTTP, LSP, MCP) to the
//! daemon's roam RPC interface. Bridges are thin protocol adapters that
//! connect as clients to the daemon.

pub mod lsp;
