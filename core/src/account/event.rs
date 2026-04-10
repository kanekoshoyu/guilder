use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::types::{OpenOrder, Position, Side, SpotBalance};

/// Events that mutate `AccountState`. Each variant carries a `timestamp`
/// so a slice of events can be sorted and replayed deterministically.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AccountEvent {
    OrderPlaced {
        timestamp: DateTime<Utc>,
        order_id: u64,
        symbol: String,
        side: Side,
        price: Decimal,
        quantity: Decimal,
        /// The strategy intent that originated this order, if placed by us.
        /// `None` for orders discovered via exchange snapshot or placed externally.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        trade_intent_uuid: Option<Uuid>,
    },
    OrderCancelled {
        timestamp: DateTime<Utc>,
        order_id: u64,
    },
    Fill {
        timestamp: DateTime<Utc>,
        order_id: u64,
        symbol: String,
        side: Side,
        price: Decimal,
        quantity: Decimal,
        fee_usd: Decimal,
        /// The strategy intent that originated this fill's order, if known.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        trade_intent_uuid: Option<Uuid>,
    },
    /// `amount_usd` is signed: negative = paid out, positive = received.
    FundingPayment {
        timestamp: DateTime<Utc>,
        symbol: String,
        amount_usd: Decimal,
    },
    Deposit {
        timestamp: DateTime<Utc>,
        amount_usd: Decimal,
    },
    Withdraw {
        timestamp: DateTime<Utc>,
        amount_usd: Decimal,
    },
    /// Full account snapshot used to seed or re-sync state from the exchange.
    ///
    /// Overwrites collateral, positions, and open orders; preserves cumulative
    /// `realized_pnl` and `funding_paid` (which are local-only accumulators).
    Snapshot {
        timestamp: DateTime<Utc>,
        collateral_usd: Decimal,
        positions: Vec<Position>,
        open_orders: Vec<OpenOrder>,
    },
    /// Spot wallet balances snapshot.
    SpotBalancesUpdated {
        timestamp: DateTime<Utc>,
        balances: Vec<SpotBalance>,
    },
}

impl AccountEvent {
    pub fn timestamp(&self) -> DateTime<Utc> {
        match self {
            AccountEvent::OrderPlaced { timestamp, .. } => *timestamp,
            AccountEvent::OrderCancelled { timestamp, .. } => *timestamp,
            AccountEvent::Fill { timestamp, .. } => *timestamp,
            AccountEvent::FundingPayment { timestamp, .. } => *timestamp,
            AccountEvent::Deposit { timestamp, .. } => *timestamp,
            AccountEvent::Withdraw { timestamp, .. } => *timestamp,
            AccountEvent::Snapshot { timestamp, .. } => *timestamp,
            AccountEvent::SpotBalancesUpdated { timestamp, .. } => *timestamp,
        }
    }
}
