/// Inbound WebSocket messages from Hyperliquid.
///
/// Each variant maps to a server → client channel. The `InboundMessage` enum
/// is parsed once in the actor; conversion to `guilder_abstraction` types
/// happens via the `as_*` methods below.
use crate::ws::parse_decimal;
use guilder_abstraction::{
    AccountBalance, AssetContext, Deposit, Fill, FundingPayment, L2Level, L2Snapshot, Liquidation,
    OrderSide, OrderStatus, OrderUpdate, UserFill, Withdrawal,
};
use serde::Deserialize;
use serde_json::Value;

// ---------------------------------------------------------------------------
// Raw payload structs (deserialization only)
// ---------------------------------------------------------------------------

#[derive(Deserialize, Clone, Debug)]
pub(crate) struct HyperliquidWsBook {
    pub(crate) coin: String,
    pub(crate) levels: Vec<Vec<HyperliquidWsLevel>>,
    pub(crate) time: i64,
}

#[derive(Deserialize, Clone, Debug)]
pub(crate) struct HyperliquidWsLevel {
    pub(crate) px: String,
    pub(crate) sz: String,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub(crate) struct HyperliquidWsAssetCtx {
    pub(crate) coin: String,
    pub(crate) ctx: HyperliquidWsPerpsCtx,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub(crate) struct HyperliquidWsPerpsCtx {
    pub(crate) open_interest: String,
    pub(crate) funding: String,
    pub(crate) mark_px: String,
    pub(crate) day_ntl_vlm: String,
    pub(crate) mid_px: Option<String>,
    pub(crate) oracle_px: Option<String>,
    pub(crate) premium: Option<String>,
    pub(crate) prev_day_px: Option<String>,
}

#[derive(Deserialize, Clone, Debug)]
pub(crate) struct HyperliquidWsUserEvent {
    pub(crate) liquidation: Option<HyperliquidWsLiquidation>,
    pub(crate) fills: Option<Vec<HyperliquidWsUserFill>>,
    pub(crate) funding: Option<HyperliquidWsFunding>,
    pub(crate) spot_state: Option<HyperliquidWsSpotState>,
}

#[derive(Deserialize, Clone, Debug)]
pub(crate) struct HyperliquidWsSpotState {
    pub(crate) balances: Option<Vec<HyperliquidWsSpotBalance>>,
}

#[derive(Deserialize, Clone, Debug)]
pub(crate) struct HyperliquidWsSpotBalance {
    pub(crate) coin: String,
    pub(crate) total: String,
    pub(crate) hold: String,
}

#[derive(Deserialize, Clone, Debug)]
pub(crate) struct HyperliquidWsLiquidation {
    pub(crate) liquidated_user: String,
    pub(crate) liquidated_ntl_pos: String,
    pub(crate) liquidated_account_value: String,
}

#[derive(Deserialize, Clone, Debug)]
pub(crate) struct HyperliquidWsUserFill {
    pub(crate) coin: String,
    pub(crate) px: String,
    pub(crate) sz: String,
    pub(crate) side: String,
    pub(crate) time: i64,
    pub(crate) oid: i64,
    pub(crate) fee: String,
    #[serde(default)]
    pub(crate) cloid: Option<String>,
}

#[derive(Deserialize, Clone, Debug)]
pub(crate) struct HyperliquidWsFunding {
    pub(crate) time: i64,
    pub(crate) coin: String,
    pub(crate) usdc: String,
}

#[derive(Deserialize, Clone, Debug)]
pub(crate) struct HyperliquidWsTrade {
    pub(crate) coin: String,
    pub(crate) side: String,
    pub(crate) px: String,
    pub(crate) sz: String,
    pub(crate) time: i64,
    pub(crate) tid: i64,
}

#[derive(Deserialize, Clone, Debug)]
pub(crate) struct HyperliquidWsOrderUpdate {
    pub(crate) order: HyperliquidWsOrderInfo,
    pub(crate) status: String,
    #[serde(rename = "statusTimestamp")]
    pub(crate) status_timestamp: i64,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub(crate) struct HyperliquidWsOrderInfo {
    pub(crate) coin: String,
    pub(crate) side: String,
    pub(crate) limit_px: String,
    pub(crate) sz: String,
    pub(crate) oid: i64,
    pub(crate) orig_sz: String,
    #[serde(default)]
    pub(crate) cloid: Option<String>,
}

#[derive(Deserialize, Clone, Debug)]
pub(crate) struct HyperliquidWsLedgerUpdates {
    #[serde(default)]
    pub(crate) updates: Vec<HyperliquidWsLedgerEntry>,
}

#[derive(Deserialize, Clone, Debug)]
pub(crate) struct HyperliquidWsLedgerEntry {
    pub(crate) time: i64,
    pub(crate) delta: HyperliquidWsLedgerDelta,
}

#[derive(Deserialize, Clone, Debug)]
pub(crate) struct HyperliquidWsLedgerDelta {
    #[serde(rename = "type")]
    pub(crate) kind: String,
    pub(crate) usdc: Option<String>,
}

// ---------------------------------------------------------------------------
// Raw envelope — the first deserialization step for any WS text message
// ---------------------------------------------------------------------------

#[derive(Deserialize, Debug)]
pub(crate) struct HyperliquidWsEnvelope {
    pub(crate) channel: String,
    #[serde(default)]
    pub(crate) data: Value,
}

#[derive(Deserialize, Clone, Debug)]
pub(crate) struct HyperliquidWsSubscriptionResponse {
    #[serde(default)]
    pub(crate) method: Option<String>,
    #[serde(default)]
    pub(crate) subscription: Value,
    #[serde(default)]
    pub(crate) success: Option<bool>,
}

// ---------------------------------------------------------------------------
// InboundMessage enum
// ---------------------------------------------------------------------------

/// One variant per Hyperliquid WS channel.
#[derive(Clone, Debug)]
pub(crate) enum HyperliquidWsInboundMessage {
    Pong,
    L2Book(HyperliquidWsBook),
    ActiveAssetCtx(HyperliquidWsAssetCtx),
    Trades(Vec<HyperliquidWsTrade>),
    User(HyperliquidWsUserEvent),
    OrderUpdates(Vec<HyperliquidWsOrderUpdate>),
    NonFundingLedger(HyperliquidWsLedgerUpdates),
    SubscriptionResponse(HyperliquidWsSubscriptionResponse),
    Unknown { channel: String, data: Value },
}

impl TryFrom<HyperliquidWsEnvelope> for HyperliquidWsInboundMessage {
    type Error = serde_json::Error;

    fn try_from(env: HyperliquidWsEnvelope) -> Result<Self, Self::Error> {
        match env.channel.as_str() {
            "pong" => Ok(Self::Pong),
            "l2Book" => serde_json::from_value(env.data).map(Self::L2Book),
            "activeAssetCtx" => serde_json::from_value(env.data).map(Self::ActiveAssetCtx),
            "trades" => serde_json::from_value(env.data).map(Self::Trades),
            "user" | "userEvents" => serde_json::from_value(env.data).map(Self::User),
            "orderUpdates" => serde_json::from_value(env.data).map(Self::OrderUpdates),
            "userNonFundingLedgerUpdates" => {
                serde_json::from_value(env.data).map(Self::NonFundingLedger)
            }
            "subscriptionResponse" => serde_json::from_value(env.data).map(Self::SubscriptionResponse),
            _ => Ok(Self::Unknown {
                channel: env.channel,
                data: env.data,
            }),
        }
    }
}

// ---------------------------------------------------------------------------
// Conversion methods — InboundMessage → guilder_abstraction types
// ---------------------------------------------------------------------------

impl HyperliquidWsInboundMessage {
    /// Extract a full L2 orderbook snapshot. Only the `L2Book` variant returns Some.
    pub fn as_l2_snapshot(&self) -> Option<L2Snapshot> {
        let HyperliquidWsInboundMessage::L2Book(book) = self else {
            return None;
        };
        let bids = book
            .levels
            .first()
            .into_iter()
            .flatten()
            .filter_map(|level| {
                Some(L2Level {
                    price: parse_decimal(&level.px)?,
                    volume: parse_decimal(&level.sz)?,
                })
            })
            .collect();
        let asks = book
            .levels
            .get(1)
            .into_iter()
            .flatten()
            .filter_map(|level| {
                Some(L2Level {
                    price: parse_decimal(&level.px)?,
                    volume: parse_decimal(&level.sz)?,
                })
            })
            .collect();
        Some(L2Snapshot {
            symbol: book.coin.clone(),
            bids,
            asks,
            sequence: book.time,
        })
    }

    /// Extract asset context. Only the `ActiveAssetCtx` variant returns Some.
    pub fn as_asset_context(&self) -> Option<AssetContext> {
        let HyperliquidWsInboundMessage::ActiveAssetCtx(update) = self else {
            return None;
        };
        let ctx = &update.ctx;
        let open_interest = parse_decimal(&ctx.open_interest)?;
        let funding_rate = parse_decimal(&ctx.funding)?;
        let mark_price = parse_decimal(&ctx.mark_px)?;
        let day_volume = parse_decimal(&ctx.day_ntl_vlm)?;
        Some(AssetContext {
            symbol: update.coin.clone(),
            open_interest,
            funding_rate,
            mark_price,
            day_volume,
            mid_price: ctx.mid_px.as_deref().and_then(parse_decimal),
            oracle_price: ctx.oracle_px.as_deref().and_then(parse_decimal),
            premium: ctx.premium.as_deref().and_then(parse_decimal),
            prev_day_price: ctx.prev_day_px.as_deref().and_then(parse_decimal),
            sz_decimals: 0,
        })
    }

    /// Extract trades as `Fill` events. Only the `Trades` variant returns Some.
    pub fn as_trades(&self) -> Option<Vec<Fill>> {
        let HyperliquidWsInboundMessage::Trades(trades) = self else {
            return None;
        };
        let result: Vec<_> = trades
            .iter()
            .filter_map(|t| {
                let side = if t.side == "B" {
                    OrderSide::Buy
                } else {
                    OrderSide::Sell
                };
                let price = parse_decimal(&t.px)?;
                let volume = parse_decimal(&t.sz)?;
                Some(Fill {
                    symbol: t.coin.clone(),
                    price,
                    volume,
                    side,
                    timestamp_ms: t.time,
                    trade_id: t.tid,
                })
            })
            .collect();
        if result.is_empty() {
            None
        } else {
            Some(result)
        }
    }

    /// Extract liquidation from a `User` event.
    pub fn as_liquidation(&self) -> Option<Liquidation> {
        let HyperliquidWsInboundMessage::User(event) = self else {
            return None;
        };
        let liq = event.liquidation.as_ref()?;
        let notional_position = parse_decimal(&liq.liquidated_ntl_pos)?;
        let account_value = parse_decimal(&liq.liquidated_account_value)?;
        Some(Liquidation {
            symbol: String::new(),
            side: OrderSide::Sell,
            liquidated_user: liq.liquidated_user.clone(),
            notional_position,
            account_value,
        })
    }

    /// Extract user fills from a `User` event.
    pub fn as_user_fills(&self) -> Option<Vec<UserFill>> {
        let HyperliquidWsInboundMessage::User(event) = self else {
            return None;
        };
        let fills = event.fills.as_ref()?;
        let result: Vec<_> = fills
            .iter()
            .filter_map(|f| {
                let side = if f.side == "B" {
                    OrderSide::Buy
                } else {
                    OrderSide::Sell
                };
                let price = parse_decimal(&f.px)?;
                let quantity = parse_decimal(&f.sz)?;
                let fee_usd = parse_decimal(&f.fee)?;
                Some(UserFill {
                    order_id: f.oid,
                    symbol: f.coin.clone(),
                    side,
                    price,
                    quantity,
                    fee_usd,
                    timestamp_ms: f.time,
                    cloid: f.cloid.clone(),
                })
            })
            .collect();
        if result.is_empty() {
            None
        } else {
            Some(result)
        }
    }

    /// Extract funding payment from a `User` event.
    pub fn as_funding_payment(&self) -> Option<FundingPayment> {
        let HyperliquidWsInboundMessage::User(event) = self else {
            return None;
        };
        let funding = event.funding.as_ref()?;
        let amount_usd = parse_decimal(&funding.usdc)?;
        Some(FundingPayment {
            symbol: funding.coin.clone(),
            amount_usd,
            timestamp_ms: funding.time,
        })
    }

    /// Extract spot balances from a `User` event's spot state.
    pub fn as_spot_balance(&self) -> Option<Vec<AccountBalance>> {
        let HyperliquidWsInboundMessage::User(event) = self else {
            return None;
        };
        let spot_state = event.spot_state.as_ref()?;
        let balances = spot_state.balances.as_ref()?;
        let result: Vec<_> = balances
            .iter()
            .filter_map(|b| {
                let equity = parse_decimal(&b.total)?;
                let hold = parse_decimal(&b.hold)?;
                let free = equity - hold;
                Some(AccountBalance {
                    token: b.coin.clone(),
                    equity,
                    free,
                    safe: None,
                    usable: free,
                    hold,
                    margin_used: None,
                    maintenance: None,
                })
            })
            .collect();
        if result.is_empty() {
            None
        } else {
            Some(result)
        }
    }

    /// Extract order updates. Only the `OrderUpdates` variant returns Some.
    pub fn as_order_updates(&self) -> Option<Vec<OrderUpdate>> {
        let HyperliquidWsInboundMessage::OrderUpdates(updates) = self else {
            return None;
        };
        let result: Vec<_> = updates
            .iter()
            .map(|u| {
                let status = match u.status.as_str() {
                    "open" => OrderStatus::Placed,
                    "filled" => OrderStatus::Filled,
                    "canceled" | "cancelled" => OrderStatus::Cancelled,
                    _ => OrderStatus::PartiallyFilled,
                };
                let side = if u.order.side == "B" {
                    OrderSide::Buy
                } else {
                    OrderSide::Sell
                };
                OrderUpdate {
                    order_id: u.order.oid,
                    symbol: u.order.coin.clone(),
                    status,
                    side: Some(side),
                    price: parse_decimal(&u.order.limit_px),
                    quantity: parse_decimal(&u.order.orig_sz),
                    remaining_quantity: parse_decimal(&u.order.sz),
                    timestamp_ms: u.status_timestamp,
                    cloid: u.order.cloid.clone(),
                }
            })
            .collect();
        if result.is_empty() {
            None
        } else {
            Some(result)
        }
    }

    /// Extract deposits from ledger updates.
    pub fn as_deposits(&self) -> Option<Vec<Deposit>> {
        let HyperliquidWsInboundMessage::NonFundingLedger(ledger) = self else {
            return None;
        };
        let result: Vec<_> = ledger
            .updates
            .iter()
            .filter_map(|e| {
                if e.delta.kind != "deposit" {
                    return None;
                }
                let amount_usd = e.delta.usdc.as_deref().and_then(parse_decimal)?;
                Some(Deposit {
                    asset: "USDC".to_string(),
                    amount_usd,
                    timestamp_ms: e.time,
                })
            })
            .collect();
        if result.is_empty() {
            None
        } else {
            Some(result)
        }
    }

    /// Extract withdrawals from ledger updates.
    pub fn as_withdrawals(&self) -> Option<Vec<Withdrawal>> {
        let HyperliquidWsInboundMessage::NonFundingLedger(ledger) = self else {
            return None;
        };
        let result: Vec<_> = ledger
            .updates
            .iter()
            .filter_map(|e| {
                if e.delta.kind != "withdraw" {
                    return None;
                }
                let amount_usd = e.delta.usdc.as_deref().and_then(parse_decimal)?;
                Some(Withdrawal {
                    asset: "USDC".to_string(),
                    amount_usd,
                    timestamp_ms: e.time,
                })
            })
            .collect();
        if result.is_empty() {
            None
        } else {
            Some(result)
        }
    }

    /// Return the channel name for SubKey routing.
    pub fn channel_name(&self) -> &str {
        match self {
            HyperliquidWsInboundMessage::Pong => "pong",
            HyperliquidWsInboundMessage::L2Book(_) => "l2Book",
            HyperliquidWsInboundMessage::ActiveAssetCtx(_) => "activeAssetCtx",
            HyperliquidWsInboundMessage::Trades(_) => "trades",
            HyperliquidWsInboundMessage::User(_) => "user",
            HyperliquidWsInboundMessage::OrderUpdates(_) => "orderUpdates",
            HyperliquidWsInboundMessage::NonFundingLedger(_) => "userNonFundingLedgerUpdates",
            HyperliquidWsInboundMessage::SubscriptionResponse(_) => "subscriptionResponse",
            HyperliquidWsInboundMessage::Unknown { channel, .. } => channel,
        }
    }

    /// Return the routing key (coin or user address) for SubKey lookup.
    pub fn routing_key(&self) -> Option<String> {
        match self {
            HyperliquidWsInboundMessage::L2Book(book) => Some(book.coin.clone()),
            HyperliquidWsInboundMessage::ActiveAssetCtx(ctx) => Some(ctx.coin.clone()),
            HyperliquidWsInboundMessage::Trades(trades) => trades.first().map(|t| t.coin.clone()),
            _ => None,
        }
    }
}
