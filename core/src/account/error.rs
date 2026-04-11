use std::fmt;

use rust_decimal::Decimal;

#[derive(Debug, Clone)]
pub enum AccountError {
    InsufficientCollateral {
        have: Decimal,
        need: Decimal,
    },
    OrderNotFound {
        order_id: u64,
    },
    PositionSizeMismatch {
        symbol: String,
        reduce: Decimal,
        have: Decimal,
    },
}

impl fmt::Display for AccountError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AccountError::InsufficientCollateral { have, need } => {
                write!(f, "insufficient collateral: have {have}, need {need}")
            }
            AccountError::OrderNotFound { order_id } => {
                write!(f, "order {order_id} not found")
            }
            AccountError::PositionSizeMismatch {
                symbol,
                reduce,
                have,
            } => {
                write!(
                    f,
                    "position size mismatch for {symbol}: reduce by {reduce} but size is {have}"
                )
            }
        }
    }
}

impl std::error::Error for AccountError {}

// ---------------------------------------------------------------------------
// Reconciliation
// ---------------------------------------------------------------------------

/// Exchange-reported account snapshot used for local ↔ exchange comparison.
#[derive(Debug, Clone)]
pub struct ExchangeSnapshot {
    pub collateral_usd: Decimal,
    /// `(symbol, signed_size)` — positive = long, negative = short.
    pub positions: Vec<(String, Decimal)>,
    pub open_order_ids: Vec<u64>,
}

#[derive(Debug, Clone)]
pub enum ReconciliationDiff {
    CollateralMismatch {
        local: Decimal,
        exchange: Decimal,
    },
    PositionMismatch {
        symbol: String,
        local: Decimal,
        exchange: Decimal,
    },
    UnknownOrder {
        order_id: u64,
    },
}
