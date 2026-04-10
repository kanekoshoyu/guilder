use std::collections::HashMap;

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use tracing::warn;
use uuid::Uuid;

use super::error::{AccountError, ExchangeSnapshot, ReconciliationDiff};
use super::event::AccountEvent;

// ---------------------------------------------------------------------------
// Primitives
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Side {
    Long,
    Short,
}

impl Side {
    pub fn flip(self) -> Self {
        match self {
            Side::Long => Side::Short,
            Side::Short => Side::Long,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrderStatus {
    Open,
    PartiallyFilled,
    Filled,
    Cancelled,
}

// ---------------------------------------------------------------------------
// Position
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    pub symbol: String,
    pub side: Side,
    /// Always positive; direction encoded by `side`.
    pub size: Decimal,
    pub entry_price: Decimal,
}

impl Position {
    /// `(mark_price - entry_price) * size` for long; negated for short.
    pub fn unrealized_pnl(&self, mark_price: Decimal) -> Decimal {
        let diff = mark_price - self.entry_price;
        match self.side {
            Side::Long => diff * self.size,
            Side::Short => -diff * self.size,
        }
    }
}

// ---------------------------------------------------------------------------
// OpenOrder
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenOrder {
    pub order_id: u64,
    pub symbol: String,
    pub side: Side,
    pub price: Decimal,
    pub quantity: Decimal,
    pub filled_quantity: Decimal,
    pub status: OrderStatus,
    /// The strategy intent that originated this order, if placed by us.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trade_intent_uuid: Option<Uuid>,
}

// ---------------------------------------------------------------------------
// RecentFill
// ---------------------------------------------------------------------------

/// A fill enriched with the originating `trade_intent_uuid` (if known).
/// Stored in a bounded ring buffer inside `AccountState` so strategy state
/// machines can observe fill outcomes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecentFill {
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub order_id: u64,
    pub symbol: String,
    pub side: Side,
    pub price: Decimal,
    pub quantity: Decimal,
    pub fee_usd: Decimal,
    /// The strategy intent that originated this fill's order, if known.
    pub trade_intent_uuid: Option<Uuid>,
}

// ---------------------------------------------------------------------------
// SpotBalance
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpotBalance {
    pub coin: String,
    pub total: Decimal,
    pub available: Decimal,
    pub locked: Decimal,
}

// ---------------------------------------------------------------------------
// AccountState
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AccountState {
    /// Available USD collateral (includes realised PnL, net of fees).
    pub collateral_usd: Decimal,
    /// Active positions keyed by symbol.
    pub positions: HashMap<String, Position>,
    /// Resting orders keyed by order_id. Removed when Filled or Cancelled.
    pub open_orders: HashMap<u64, OpenOrder>,
    /// Cumulative realised PnL across all closed positions.
    pub realized_pnl: Decimal,
    /// Cumulative net funding (negative = net paid out).
    pub funding_paid: Decimal,
    /// Recent fills with intent context (bounded ring buffer, newest last).
    #[serde(default)]
    pub recent_fills: Vec<RecentFill>,
    /// Spot wallet balances keyed by coin symbol.
    #[serde(default)]
    pub spot_balances: HashMap<String, SpotBalance>,
}

impl AccountState {
    // -----------------------------------------------------------------------
    // Single mutation entry point
    // -----------------------------------------------------------------------

    /// Apply one event, mutating state in place.
    ///
    /// This is the **only** method that writes to `AccountState`.
    pub fn apply(&mut self, event: &AccountEvent) -> Result<(), AccountError> {
        match event {
            AccountEvent::OrderPlaced { order_id, symbol, side, price, quantity, trade_intent_uuid, .. } => {
                self.open_orders.insert(
                    *order_id,
                    OpenOrder {
                        order_id: *order_id,
                        symbol: symbol.clone(),
                        side: *side,
                        price: *price,
                        quantity: *quantity,
                        filled_quantity: Decimal::ZERO,
                        status: OrderStatus::Open,
                        trade_intent_uuid: *trade_intent_uuid,
                    },
                );
            }

            AccountEvent::OrderCancelled { order_id, .. } => {
                // Idempotent: no-op if order unknown.
                self.open_orders.remove(order_id);
            }

            AccountEvent::Fill { timestamp, order_id, symbol, side, price, quantity, fee_usd, trade_intent_uuid } => {
                if self.collateral_usd < *fee_usd {
                    return Err(AccountError::InsufficientCollateral {
                        have: self.collateral_usd,
                        need: *fee_usd,
                    });
                }
                self.collateral_usd -= fee_usd;

                let remove_position = if let Some(pos) = self.positions.get_mut(symbol) {
                    if pos.side == *side {
                        // Same side: weighted-average entry price.
                        let new_size = pos.size + quantity;
                        pos.entry_price =
                            (pos.size * pos.entry_price + quantity * price) / new_size;
                        pos.size = new_size;
                        false
                    } else {
                        // Opposite side: reduce / close position.
                        if *quantity > pos.size {
                            return Err(AccountError::PositionSizeMismatch {
                                symbol: symbol.clone(),
                                reduce: *quantity,
                                have: pos.size,
                            });
                        }
                        let pnl = match pos.side {
                            Side::Long => (price - pos.entry_price) * quantity,
                            Side::Short => (pos.entry_price - price) * quantity,
                        };
                        self.realized_pnl += pnl;
                        self.collateral_usd += pnl;
                        pos.size -= quantity;
                        pos.size.is_zero()
                    }
                } else {
                    self.positions.insert(
                        symbol.clone(),
                        Position {
                            symbol: symbol.clone(),
                            side: *side,
                            size: *quantity,
                            entry_price: *price,
                        },
                    );
                    false
                };

                if remove_position {
                    self.positions.remove(symbol);
                }

                let remove_order = if let Some(order) = self.open_orders.get_mut(order_id) {
                    order.filled_quantity += quantity;
                    order.status = if order.filled_quantity >= order.quantity {
                        OrderStatus::Filled
                    } else {
                        OrderStatus::PartiallyFilled
                    };
                    order.status == OrderStatus::Filled
                } else {
                    false
                };

                if remove_order {
                    self.open_orders.remove(order_id);
                }

                // Record fill with intent context for strategy feedback.
                let resolved_uuid = trade_intent_uuid.or_else(|| {
                    self.open_orders
                        .get(order_id)
                        .and_then(|o| o.trade_intent_uuid)
                });
                self.recent_fills.push(RecentFill {
                    timestamp: *timestamp,
                    order_id: *order_id,
                    symbol: symbol.clone(),
                    side: *side,
                    price: *price,
                    quantity: *quantity,
                    fee_usd: *fee_usd,
                    trade_intent_uuid: resolved_uuid,
                });
                // Keep bounded: retain only the last 200 fills.
                const MAX_RECENT_FILLS: usize = 200;
                if self.recent_fills.len() > MAX_RECENT_FILLS {
                    self.recent_fills.drain(..self.recent_fills.len() - MAX_RECENT_FILLS);
                }
            }

            AccountEvent::FundingPayment { amount_usd, .. } => {
                self.collateral_usd += amount_usd;
                self.funding_paid += amount_usd;
            }

            AccountEvent::Deposit { amount_usd, .. } => {
                self.collateral_usd += amount_usd;
            }

            AccountEvent::Withdraw { amount_usd, .. } => {
                if self.collateral_usd < *amount_usd {
                    return Err(AccountError::InsufficientCollateral {
                        have: self.collateral_usd,
                        need: *amount_usd,
                    });
                }
                self.collateral_usd -= amount_usd;
            }

            AccountEvent::Snapshot { collateral_usd, positions, open_orders, .. } => {
                self.collateral_usd = *collateral_usd;
                self.positions = positions.iter().map(|p| (p.symbol.clone(), p.clone())).collect();
                self.open_orders = open_orders.iter().map(|o| (o.order_id, o.clone())).collect();
            }

            AccountEvent::SpotBalancesUpdated { balances, .. } => {
                self.spot_balances = balances.iter().map(|b| (b.coin.clone(), b.clone())).collect();
            }
        }

        Ok(())
    }

    // -----------------------------------------------------------------------
    // Replay constructor
    // -----------------------------------------------------------------------

    /// Reconstruct state from an ordered event slice.
    pub fn from_events(events: &[AccountEvent]) -> Self {
        let mut state = Self::default();
        for event in events {
            if let Err(e) = state.apply(event) {
                warn!("from_events: skipping event due to apply error: {e}");
            }
        }
        state
    }

    // -----------------------------------------------------------------------
    // Checkpoint
    // -----------------------------------------------------------------------

    pub fn checkpoint(&self) -> Self {
        self.clone()
    }

    // -----------------------------------------------------------------------
    // Derived metrics (pure &self)
    // -----------------------------------------------------------------------

    pub fn equity(&self, mark_prices: &HashMap<String, Decimal>) -> Decimal {
        let unrealized: Decimal = self
            .positions
            .values()
            .map(|p| {
                mark_prices
                    .get(&p.symbol)
                    .map(|&mp| p.unrealized_pnl(mp))
                    .unwrap_or(Decimal::ZERO)
            })
            .sum();
        self.collateral_usd + unrealized
    }

    pub fn margin_used(&self) -> Decimal {
        self.positions.values().map(|p| p.size * p.entry_price).sum()
    }

    pub fn free_collateral(&self, mark_prices: &HashMap<String, Decimal>) -> Decimal {
        self.equity(mark_prices) - self.margin_used()
    }

    // -----------------------------------------------------------------------
    // Reconciliation
    // -----------------------------------------------------------------------

    pub fn reconcile(&self, snapshot: &ExchangeSnapshot) -> Vec<ReconciliationDiff> {
        let mut diffs = Vec::new();

        if self.collateral_usd != snapshot.collateral_usd {
            diffs.push(ReconciliationDiff::CollateralMismatch {
                local: self.collateral_usd,
                exchange: snapshot.collateral_usd,
            });
        }

        let snapshot_map: HashMap<&str, Decimal> = snapshot
            .positions
            .iter()
            .map(|(s, sz)| (s.as_str(), *sz))
            .collect();

        for (symbol, &exchange_size) in &snapshot_map {
            let local_size = self
                .positions
                .get(*symbol)
                .map(|p| match p.side {
                    Side::Long => p.size,
                    Side::Short => -p.size,
                })
                .unwrap_or(Decimal::ZERO);

            if local_size != exchange_size {
                diffs.push(ReconciliationDiff::PositionMismatch {
                    symbol: symbol.to_string(),
                    local: local_size,
                    exchange: exchange_size,
                });
            }
        }

        // Ghost positions: local has, exchange doesn't.
        for (symbol, pos) in &self.positions {
            if !snapshot_map.contains_key(symbol.as_str()) && !pos.size.is_zero() {
                let local_size = match pos.side {
                    Side::Long => pos.size,
                    Side::Short => -pos.size,
                };
                diffs.push(ReconciliationDiff::PositionMismatch {
                    symbol: symbol.clone(),
                    local: local_size,
                    exchange: Decimal::ZERO,
                });
            }
        }

        for order_id in &snapshot.open_order_ids {
            if !self.open_orders.contains_key(order_id) {
                diffs.push(ReconciliationDiff::UnknownOrder { order_id: *order_id });
            }
        }

        diffs
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use rust_decimal_macros::dec;

    use super::*;

    fn ts() -> chrono::DateTime<Utc> {
        Utc::now()
    }

    // --- OrderPlaced / OrderCancelled ---

    #[test]
    fn order_placed_adds_to_open_orders() {
        let mut s = AccountState::default();
        s.apply(&AccountEvent::OrderPlaced {
            timestamp: ts(), order_id: 1, symbol: "BTC".into(),
            side: Side::Long, price: dec!(50000), quantity: dec!(1), trade_intent_uuid: None,
        }).unwrap();
        assert_eq!(s.open_orders.len(), 1);
        let o = &s.open_orders[&1];
        assert_eq!(o.status, OrderStatus::Open);
        assert_eq!(o.filled_quantity, Decimal::ZERO);
    }

    #[test]
    fn order_cancelled_removes_order_and_is_idempotent() {
        let mut s = AccountState::default();
        s.apply(&AccountEvent::OrderPlaced {
            timestamp: ts(), order_id: 1, symbol: "BTC".into(),
            side: Side::Long, price: dec!(50000), quantity: dec!(1), trade_intent_uuid: None,
        }).unwrap();
        s.apply(&AccountEvent::OrderCancelled { timestamp: ts(), order_id: 1 }).unwrap();
        assert!(s.open_orders.is_empty());
        s.apply(&AccountEvent::OrderCancelled { timestamp: ts(), order_id: 1 }).unwrap();
    }

    // --- Fill: open new position ---

    #[test]
    fn fill_opens_long_position_and_deducts_fee() {
        let mut s = AccountState::default();
        s.collateral_usd = dec!(1000);
        s.apply(&AccountEvent::OrderPlaced {
            timestamp: ts(), order_id: 1, symbol: "ETH".into(),
            side: Side::Long, price: dec!(2000), quantity: dec!(1), trade_intent_uuid: None,
        }).unwrap();
        s.apply(&AccountEvent::Fill {
            timestamp: ts(), order_id: 1, symbol: "ETH".into(),
            side: Side::Long, price: dec!(2000), quantity: dec!(1), fee_usd: dec!(2), trade_intent_uuid: None,
        }).unwrap();

        assert_eq!(s.collateral_usd, dec!(998));
        let pos = s.positions.get("ETH").unwrap();
        assert_eq!(pos.side, Side::Long);
        assert_eq!(pos.size, dec!(1));
        assert_eq!(pos.entry_price, dec!(2000));
        assert!(s.open_orders.is_empty(), "fully-filled order should be removed");
    }

    // --- Fill: weighted average entry ---

    #[test]
    fn fill_averages_entry_price_on_same_side_add() {
        let mut s = AccountState::default();
        s.collateral_usd = dec!(10000);
        s.apply(&AccountEvent::Fill {
            timestamp: ts(), order_id: 1, symbol: "BTC".into(),
            side: Side::Long, price: dec!(40000), quantity: dec!(1), fee_usd: Decimal::ZERO, trade_intent_uuid: None,
        }).unwrap();
        s.apply(&AccountEvent::Fill {
            timestamp: ts(), order_id: 2, symbol: "BTC".into(),
            side: Side::Long, price: dec!(60000), quantity: dec!(1), fee_usd: Decimal::ZERO, trade_intent_uuid: None,
        }).unwrap();

        let pos = s.positions.get("BTC").unwrap();
        assert_eq!(pos.size, dec!(2));
        assert_eq!(pos.entry_price, dec!(50000)); // (40000*1 + 60000*1) / 2
    }

    // --- Fill: position reduction and PnL ---

    #[test]
    fn fill_reduces_long_and_realises_pnl() {
        let mut s = AccountState::default();
        s.collateral_usd = dec!(10000);
        s.apply(&AccountEvent::Fill {
            timestamp: ts(), order_id: 1, symbol: "BTC".into(),
            side: Side::Long, price: dec!(40000), quantity: dec!(2), fee_usd: Decimal::ZERO, trade_intent_uuid: None,
        }).unwrap();
        s.apply(&AccountEvent::Fill {
            timestamp: ts(), order_id: 2, symbol: "BTC".into(),
            side: Side::Short, price: dec!(50000), quantity: dec!(1), fee_usd: Decimal::ZERO, trade_intent_uuid: None,
        }).unwrap();

        assert_eq!(s.realized_pnl, dec!(10000));
        assert_eq!(s.collateral_usd, dec!(20000)); // 10000 + 10000 pnl
        let pos = s.positions.get("BTC").unwrap();
        assert_eq!(pos.size, dec!(1));
    }

    #[test]
    fn fill_closes_position_entirely_removes_it() {
        let mut s = AccountState::default();
        s.collateral_usd = dec!(10000);
        s.apply(&AccountEvent::Fill {
            timestamp: ts(), order_id: 1, symbol: "BTC".into(),
            side: Side::Long, price: dec!(50000), quantity: dec!(1), fee_usd: Decimal::ZERO, trade_intent_uuid: None,
        }).unwrap();
        s.apply(&AccountEvent::Fill {
            timestamp: ts(), order_id: 2, symbol: "BTC".into(),
            side: Side::Short, price: dec!(50000), quantity: dec!(1), fee_usd: Decimal::ZERO, trade_intent_uuid: None,
        }).unwrap();

        assert!(s.positions.is_empty());
        assert_eq!(s.realized_pnl, Decimal::ZERO); // bought and sold at same price
    }

    #[test]
    fn fill_position_size_mismatch_returns_err() {
        let mut s = AccountState::default();
        s.collateral_usd = dec!(10000);
        s.apply(&AccountEvent::Fill {
            timestamp: ts(), order_id: 1, symbol: "BTC".into(),
            side: Side::Long, price: dec!(50000), quantity: dec!(1), fee_usd: Decimal::ZERO, trade_intent_uuid: None,
        }).unwrap();
        let result = s.apply(&AccountEvent::Fill {
            timestamp: ts(), order_id: 2, symbol: "BTC".into(),
            side: Side::Short, price: dec!(50000), quantity: dec!(2), fee_usd: Decimal::ZERO, trade_intent_uuid: None,
        });
        assert!(matches!(result, Err(AccountError::PositionSizeMismatch { .. })));
    }

    #[test]
    fn fill_insufficient_collateral_for_fee_returns_err() {
        let mut s = AccountState::default();
        s.collateral_usd = dec!(1);
        let result = s.apply(&AccountEvent::Fill {
            timestamp: ts(), order_id: 1, symbol: "ETH".into(),
            side: Side::Long, price: dec!(2000), quantity: dec!(1), fee_usd: dec!(5), trade_intent_uuid: None,
        });
        assert!(matches!(result, Err(AccountError::InsufficientCollateral { .. })));
    }

    // --- Partial fill order status ---

    #[test]
    fn partial_fill_updates_order_status_to_partially_filled() {
        let mut s = AccountState::default();
        s.collateral_usd = dec!(10000);
        s.apply(&AccountEvent::OrderPlaced {
            timestamp: ts(), order_id: 1, symbol: "BTC".into(),
            side: Side::Long, price: dec!(50000), quantity: dec!(2), trade_intent_uuid: None,
        }).unwrap();
        s.apply(&AccountEvent::Fill {
            timestamp: ts(), order_id: 1, symbol: "BTC".into(),
            side: Side::Long, price: dec!(50000), quantity: dec!(1), fee_usd: Decimal::ZERO, trade_intent_uuid: None,
        }).unwrap();

        let o = &s.open_orders[&1];
        assert_eq!(o.status, OrderStatus::PartiallyFilled);
        assert_eq!(o.filled_quantity, dec!(1));
    }

    // --- Funding ---

    #[test]
    fn funding_payment_adjusts_collateral_and_accumulates() {
        let mut s = AccountState::default();
        s.collateral_usd = dec!(1000);
        s.apply(&AccountEvent::FundingPayment {
            timestamp: ts(), symbol: "BTC".into(), amount_usd: dec!(-10),
        }).unwrap();
        assert_eq!(s.collateral_usd, dec!(990));
        assert_eq!(s.funding_paid, dec!(-10));

        s.apply(&AccountEvent::FundingPayment {
            timestamp: ts(), symbol: "BTC".into(), amount_usd: dec!(3),
        }).unwrap();
        assert_eq!(s.collateral_usd, dec!(993));
        assert_eq!(s.funding_paid, dec!(-7));
    }

    // --- Deposit / Withdraw ---

    #[test]
    fn deposit_and_withdraw_adjust_collateral() {
        let mut s = AccountState::default();
        s.apply(&AccountEvent::Deposit { timestamp: ts(), amount_usd: dec!(500) }).unwrap();
        assert_eq!(s.collateral_usd, dec!(500));

        s.apply(&AccountEvent::Withdraw { timestamp: ts(), amount_usd: dec!(200) }).unwrap();
        assert_eq!(s.collateral_usd, dec!(300));
    }

    #[test]
    fn withdraw_below_zero_returns_err() {
        let mut s = AccountState::default();
        s.collateral_usd = dec!(100);
        let result = s.apply(&AccountEvent::Withdraw { timestamp: ts(), amount_usd: dec!(200) });
        assert!(matches!(result, Err(AccountError::InsufficientCollateral { .. })));
        assert_eq!(s.collateral_usd, dec!(100));
    }

    // --- Snapshot ---

    #[test]
    fn snapshot_seeds_state_preserves_accumulators() {
        let mut s = AccountState::default();
        s.realized_pnl = dec!(100);
        s.funding_paid = dec!(-5);
        s.apply(&AccountEvent::Snapshot {
            timestamp: ts(),
            collateral_usd: dec!(2000),
            positions: vec![Position { symbol: "ETH".into(), side: Side::Long, size: dec!(1), entry_price: dec!(1800) }],
            open_orders: vec![],
        }).unwrap();

        assert_eq!(s.collateral_usd, dec!(2000));
        assert_eq!(s.positions.len(), 1);
        assert_eq!(s.realized_pnl, dec!(100));
        assert_eq!(s.funding_paid, dec!(-5));
    }

    // --- from_events replay ---

    #[test]
    fn from_events_replay_matches_sequential_apply() {
        let events = vec![
            AccountEvent::Deposit { timestamp: ts(), amount_usd: dec!(1000) },
            AccountEvent::Fill {
                timestamp: ts(), order_id: 1, symbol: "BTC".into(),
                side: Side::Long, price: dec!(50000), quantity: dec!(1), fee_usd: dec!(1), trade_intent_uuid: None,
            },
            AccountEvent::FundingPayment { timestamp: ts(), symbol: "BTC".into(), amount_usd: dec!(-2) },
        ];

        let mut expected = AccountState::default();
        for e in &events { expected.apply(e).unwrap(); }

        let replayed = AccountState::from_events(&events);
        assert_eq!(replayed.collateral_usd, expected.collateral_usd);
        assert_eq!(replayed.funding_paid, expected.funding_paid);
        assert_eq!(replayed.positions.len(), expected.positions.len());
    }

    // --- reconcile ---

    #[test]
    fn reconcile_detects_collateral_mismatch() {
        let mut s = AccountState::default();
        s.collateral_usd = dec!(1000);
        let snap = ExchangeSnapshot { collateral_usd: dec!(999), positions: vec![], open_order_ids: vec![] };
        let diffs = s.reconcile(&snap);
        assert!(diffs.iter().any(|d| matches!(d, ReconciliationDiff::CollateralMismatch { .. })));
    }

    #[test]
    fn reconcile_detects_ghost_position() {
        let mut s = AccountState::default();
        s.positions.insert("BTC".into(), Position {
            symbol: "BTC".into(), side: Side::Long, size: dec!(1), entry_price: dec!(50000),
        });
        let snap = ExchangeSnapshot { collateral_usd: Decimal::ZERO, positions: vec![], open_order_ids: vec![] };
        let diffs = s.reconcile(&snap);
        assert!(diffs.iter().any(|d| matches!(d, ReconciliationDiff::PositionMismatch { .. })));
    }

    #[test]
    fn reconcile_clean_state_produces_no_diffs() {
        let mut s = AccountState::default();
        s.collateral_usd = dec!(500);
        let snap = ExchangeSnapshot { collateral_usd: dec!(500), positions: vec![], open_order_ids: vec![] };
        assert!(s.reconcile(&snap).is_empty());
    }

    // --- Short position lifecycle ---

    #[test]
    fn fill_opens_short_position() {
        let mut s = AccountState::default();
        s.collateral_usd = dec!(1000);
        s.apply(&AccountEvent::Fill {
            timestamp: ts(), order_id: 1, symbol: "ETH".into(),
            side: Side::Short, price: dec!(2000), quantity: dec!(1), fee_usd: dec!(2), trade_intent_uuid: None,
        }).unwrap();

        let pos = s.positions.get("ETH").unwrap();
        assert_eq!(pos.side, Side::Short);
        assert_eq!(pos.size, dec!(1));
        assert_eq!(pos.entry_price, dec!(2000));
        assert_eq!(s.collateral_usd, dec!(998));
    }

    #[test]
    fn fill_reduces_short_and_realises_pnl() {
        let mut s = AccountState::default();
        s.collateral_usd = dec!(10000);
        s.apply(&AccountEvent::Fill {
            timestamp: ts(), order_id: 1, symbol: "BTC".into(),
            side: Side::Short, price: dec!(50000), quantity: dec!(2), fee_usd: Decimal::ZERO, trade_intent_uuid: None,
        }).unwrap();
        s.apply(&AccountEvent::Fill {
            timestamp: ts(), order_id: 2, symbol: "BTC".into(),
            side: Side::Long, price: dec!(40000), quantity: dec!(1), fee_usd: Decimal::ZERO, trade_intent_uuid: None,
        }).unwrap();

        assert_eq!(s.realized_pnl, dec!(10000));
        assert_eq!(s.collateral_usd, dec!(20000));
        let pos = s.positions.get("BTC").unwrap();
        assert_eq!(pos.side, Side::Short);
        assert_eq!(pos.size, dec!(1));
    }

    #[test]
    fn fill_closes_short_position_entirely() {
        let mut s = AccountState::default();
        s.collateral_usd = dec!(10000);
        s.apply(&AccountEvent::Fill {
            timestamp: ts(), order_id: 1, symbol: "BTC".into(),
            side: Side::Short, price: dec!(50000), quantity: dec!(1), fee_usd: Decimal::ZERO, trade_intent_uuid: None,
        }).unwrap();
        s.apply(&AccountEvent::Fill {
            timestamp: ts(), order_id: 2, symbol: "BTC".into(),
            side: Side::Long, price: dec!(60000), quantity: dec!(1), fee_usd: Decimal::ZERO, trade_intent_uuid: None,
        }).unwrap();

        assert!(s.positions.is_empty());
        assert_eq!(s.realized_pnl, dec!(-10000));
    }

    // --- Spot balances ---

    #[test]
    fn spot_balances_updated() {
        let mut s = AccountState::default();
        s.apply(&AccountEvent::SpotBalancesUpdated {
            timestamp: ts(),
            balances: vec![
                SpotBalance { coin: "USDC".into(), total: dec!(5000), available: dec!(4000), locked: dec!(1000) },
            ],
        }).unwrap();

        let usdc = s.spot_balances.get("USDC").unwrap();
        assert_eq!(usdc.total, dec!(5000));
        assert_eq!(usdc.available, dec!(4000));
        assert_eq!(usdc.locked, dec!(1000));
    }

    // --- RecentFill ring buffer ---

    #[test]
    fn recent_fills_bounded_at_200() {
        let mut s = AccountState::default();
        s.collateral_usd = dec!(100000);
        for i in 0..250 {
            s.apply(&AccountEvent::Fill {
                timestamp: ts(), order_id: i, symbol: "BTC".into(),
                side: Side::Long, price: dec!(50000), quantity: dec!(1), fee_usd: Decimal::ZERO, trade_intent_uuid: None,
            }).unwrap();
        }
        assert_eq!(s.recent_fills.len(), 200);
        assert_eq!(s.recent_fills[0].order_id, 50);
        assert_eq!(s.recent_fills.last().unwrap().order_id, 249);
    }

    // --- Fill intent uuid resolution ---

    #[test]
    fn fill_records_intent_uuid_in_recent_fill() {
        let mut s = AccountState::default();
        s.collateral_usd = dec!(1000);
        let intent_id = Uuid::new_v4();
        s.apply(&AccountEvent::OrderPlaced {
            timestamp: ts(), order_id: 1, symbol: "BTC".into(),
            side: Side::Long, price: dec!(50000), quantity: dec!(2), trade_intent_uuid: Some(intent_id),
        }).unwrap();
        s.apply(&AccountEvent::Fill {
            timestamp: ts(), order_id: 1, symbol: "BTC".into(),
            side: Side::Long, price: dec!(50000), quantity: dec!(1), fee_usd: Decimal::ZERO, trade_intent_uuid: None,
        }).unwrap();

        let fill = s.recent_fills.last().unwrap();
        assert_eq!(fill.trade_intent_uuid, Some(intent_id));
    }

    // --- Multiple symbols ---

    #[test]
    fn multiple_positions_for_different_symbols() {
        let mut s = AccountState::default();
        s.collateral_usd = dec!(10000);
        s.apply(&AccountEvent::Fill {
            timestamp: ts(), order_id: 1, symbol: "BTC".into(),
            side: Side::Long, price: dec!(50000), quantity: dec!(1), fee_usd: Decimal::ZERO, trade_intent_uuid: None,
        }).unwrap();
        s.apply(&AccountEvent::Fill {
            timestamp: ts(), order_id: 2, symbol: "ETH".into(),
            side: Side::Long, price: dec!(2000), quantity: dec!(2), fee_usd: Decimal::ZERO, trade_intent_uuid: None,
        }).unwrap();

        assert_eq!(s.positions.len(), 2);
        assert!(s.positions.contains_key("BTC"));
        assert!(s.positions.contains_key("ETH"));
    }

    // --- Snapshot overwrites existing state ---

    #[test]
    fn snapshot_overwrites_positions_and_orders() {
        let mut s = AccountState::default();
        s.collateral_usd = dec!(1000);
        s.positions.insert("BTC".into(), Position { symbol: "BTC".into(), side: Side::Long, size: dec!(1), entry_price: dec!(50000) });
        s.open_orders.insert(1, OpenOrder { order_id: 1, symbol: "BTC".into(), side: Side::Long, price: dec!(50000), quantity: dec!(1), filled_quantity: Decimal::ZERO, status: OrderStatus::Open, trade_intent_uuid: None });

        s.apply(&AccountEvent::Snapshot {
            timestamp: ts(),
            collateral_usd: dec!(2000),
            positions: vec![Position { symbol: "ETH".into(), side: Side::Short, size: dec!(5), entry_price: dec!(3000) }],
            open_orders: vec![],
        }).unwrap();

        assert_eq!(s.collateral_usd, dec!(2000));
        assert!(!s.positions.contains_key("BTC"));
        assert!(s.positions.contains_key("ETH"));
        assert!(s.open_orders.is_empty());
    }

    // --- Derived metrics ---

    #[test]
    fn equity_includes_unrealized_pnl() {
        let mut s = AccountState::default();
        s.collateral_usd = dec!(10000);
        s.apply(&AccountEvent::Fill {
            timestamp: ts(), order_id: 1, symbol: "BTC".into(),
            side: Side::Long, price: dec!(50000), quantity: dec!(1), fee_usd: Decimal::ZERO, trade_intent_uuid: None,
        }).unwrap();

        let mut prices = HashMap::new();
        prices.insert("BTC".to_string(), dec!(52000));
        let equity = s.equity(&prices);
        assert_eq!(equity, dec!(12000));
    }

    #[test]
    fn margin_used_returns_notional() {
        let mut s = AccountState::default();
        s.collateral_usd = dec!(10000);
        s.apply(&AccountEvent::Fill {
            timestamp: ts(), order_id: 1, symbol: "BTC".into(),
            side: Side::Long, price: dec!(50000), quantity: dec!(2), fee_usd: Decimal::ZERO, trade_intent_uuid: None,
        }).unwrap();

        assert_eq!(s.margin_used(), dec!(100000));
    }

    // --- Reconciliation ---

    #[test]
    fn reconcile_detects_unknown_order() {
        let mut s = AccountState::default();
        s.collateral_usd = dec!(500);
        let snap = ExchangeSnapshot {
            collateral_usd: dec!(500),
            positions: vec![],
            open_order_ids: vec![999],
        };
        let diffs = s.reconcile(&snap);
        assert!(diffs.iter().any(|d| matches!(d, ReconciliationDiff::UnknownOrder { order_id: 999 })));
    }

    #[test]
    fn fill_without_prior_order_still_creates_position() {
        let mut s = AccountState::default();
        s.collateral_usd = dec!(1000);
        s.apply(&AccountEvent::Fill {
            timestamp: ts(), order_id: 999, symbol: "ETH".into(),
            side: Side::Long, price: dec!(2000), quantity: dec!(1), fee_usd: Decimal::ZERO, trade_intent_uuid: None,
        }).unwrap();

        let pos = s.positions.get("ETH").unwrap();
        assert_eq!(pos.size, dec!(1));
    }
}
