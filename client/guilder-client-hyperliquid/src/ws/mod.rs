/// WebSocket module for Hyperliquid.
///
/// - `inbound.rs` — `InboundMessage` enum, raw payload structs, conversion to guilder types
/// - `outbound.rs` — `OutboundMessage` enum, JSON serialization
/// - `transport.rs` — `WsTransport` trait + `HyperliquidWs` impl
/// - `session.rs` — `WsSession` session management between client and transport
/// - `sub_key.rs` — `SubKey` subscription routing key
mod inbound;
mod outbound;
mod transport;
mod session;
mod sub_key;

pub(crate) use session::WsSession;
pub(crate) use sub_key::SubKey;
pub(crate) use outbound::HyperliquidWsOutboundMessage;
pub(crate) use inbound::HyperliquidWsBook;

use std::str::FromStr;

fn parse_decimal(s: &str) -> Option<rust_decimal::Decimal> {
    rust_decimal::Decimal::from_str(s).ok()
}
