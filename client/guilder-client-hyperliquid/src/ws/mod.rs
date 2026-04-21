/// WebSocket module for Hyperliquid.
///
/// - `inbound.rs` — `InboundMessage` enum, raw payload structs, conversion to guilder types
/// - `outbound.rs` — `OutboundMessage` enum, JSON serialization
/// - `transport.rs` — `WsTransport` trait + `HyperliquidWs` impl
pub(crate) mod inbound;
pub(crate) mod outbound;
pub(crate) mod transport;

pub(crate) use outbound::HyperliquidWsOutboundMessage;
pub(crate) use inbound::{HyperliquidWsBook, HyperliquidWsInboundMessage};

use std::str::FromStr;

fn parse_decimal(s: &str) -> Option<rust_decimal::Decimal> {
    rust_decimal::Decimal::from_str(s).ok()
}
