/// Outbound WebSocket messages sent to Hyperliquid.
///
/// Covers all client → server message types. Each variant maps to a
/// concrete Hyperliquid wire format via `to_json()`.
use serde::Serialize;

/// Cancel action for batch cancel orders via WS.
#[derive(Serialize)]
pub(crate) struct CancelAction {
    pub(crate) a: usize,
    pub(crate) o: i64,
}

/// One variant per client → server message type.
pub(crate) enum HyperliquidWsOutboundMessage {
    /// Subscribe to an l2Book channel for a specific coin.
    SubscribeL2Book { coin: String },
    /// Subscribe to the trades channel for a specific coin.
    SubscribeTrades { coin: String },
    /// Subscribe to the activeAssetCtx channel for a specific coin.
    SubscribeActiveAssetCtx { coin: String },
    /// Subscribe to the user events channel for a specific address.
    SubscribeUserEvents { user_addr: String },
    /// Subscribe to the orderUpdates channel for a specific address.
    SubscribeOrderUpdates { user_addr: String },
    /// Subscribe to the userNonFundingLedgerUpdates channel for a specific address.
    SubcribeNonFundingLedger { user_addr: String },
    /// Unsubscribe from any channel (channel + subscription JSON).
    #[allow(dead_code)]
    Unsubscribe {
        channel: String,
        subscription: serde_json::Value,
    },
    /// Keepalive ping.
    Ping,
    /// Place an order via WS (uses the same signed action payload as REST).
    #[allow(dead_code)]
    PlaceOrder { payload: serde_json::Value },
    /// Cancel orders via WS.
    #[allow(dead_code)]
    CancelOrder { cancels: Vec<CancelAction> },
}

impl HyperliquidWsOutboundMessage {
    /// Serialize to the JSON text format expected by Hyperliquid's WS endpoint.
    pub fn to_json(&self) -> String {
        match self {
            HyperliquidWsOutboundMessage::SubscribeL2Book { coin } => serde_json::json!({
                "method": "subscribe",
                "subscription": {
                    "type": "l2Book",
                    "coin": coin,
                }
            })
            .to_string(),
            HyperliquidWsOutboundMessage::SubscribeTrades { coin } => serde_json::json!({
                "method": "subscribe",
                "subscription": {
                    "type": "trades",
                    "coin": coin,
                }
            })
            .to_string(),
            HyperliquidWsOutboundMessage::SubscribeActiveAssetCtx { coin } => serde_json::json!({
                "method": "subscribe",
                "subscription": {
                    "type": "activeAssetCtx",
                    "coin": coin,
                }
            })
            .to_string(),
            HyperliquidWsOutboundMessage::SubscribeUserEvents { user_addr } => serde_json::json!({
                "method": "subscribe",
                "subscription": {
                    "type": "user",
                    "user": user_addr,
                }
            })
            .to_string(),
            HyperliquidWsOutboundMessage::SubscribeOrderUpdates { user_addr } => {
                serde_json::json!({
                    "method": "subscribe",
                    "subscription": {
                        "type": "orderUpdates",
                        "user": user_addr,
                    }
                })
                .to_string()
            }
            HyperliquidWsOutboundMessage::SubcribeNonFundingLedger { user_addr } => {
                serde_json::json!({
                    "method": "subscribe",
                    "subscription": {
                        "type": "userNonFundingLedgerUpdates",
                        "user": user_addr,
                    }
                })
                .to_string()
            }
            HyperliquidWsOutboundMessage::Unsubscribe { subscription, .. } => serde_json::json!({
                "method": "unsubscribe",
                "subscription": subscription
            })
            .to_string(),
            HyperliquidWsOutboundMessage::Ping => r#"{"method":"ping"}"#.to_string(),
            HyperliquidWsOutboundMessage::PlaceOrder { payload } => payload.to_string(),
            HyperliquidWsOutboundMessage::CancelOrder { cancels } => serde_json::json!({
                "type": "cancel",
                "cancels": cancels
            })
            .to_string(),
        }
    }
}
