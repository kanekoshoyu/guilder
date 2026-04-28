use futures_core::Stream;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::pin::Pin;

pub type BoxStream<T> = Pin<Box<dyn Stream<Item = T> + Send + 'static>>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Status {
    /// task succeeded
    Success,
    /// task currently in progress
    InProgress,
    /// task completed
    Completed,
    /// task failed
    Failed,
}

/// orderbook side
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Side {
    /// bid side
    Bid,
    /// ask side
    Ask,
}

/// direction of an order
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum OrderSide {
    /// buy order
    Buy,
    /// sell order
    Sell,
}

/// lifecycle state of an order
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum OrderStatus {
    /// order accepted by exchange
    Placed,
    /// order partially filled
    PartiallyFilled,
    /// order fully filled
    Filled,
    /// order cancelled
    Cancelled,
}

/// type of market
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum MarketType {
    /// spot market
    Spot,
    /// dated futures
    Future,
    /// perpetual futures
    Perpetual,
}

/// order execution type
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum OrderType {
    /// execute immediately at market price
    Market,
    /// execute at specified price or better
    Limit,
    /// trigger order that fires as market/limit when price reaches take-profit level
    TakeProfit,
    /// trigger order that fires as market/limit when price reaches stop-loss level
    StopLoss,
}

/// how long an order remains active
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TimeInForce {
    /// good till cancel
    Gtc,
    /// immediate or cancel
    Ioc,
    /// fill or kill
    Fok,
    /// add liquidity only (post-only, never taker)
    Alo,
}

/// which currency the volume is expressed in
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum VolumeDenomination {
    /// base currency
    Base,
    /// quote currency
    Quote,
}

/// broad class of asset
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AssetClass {
    /// cryptocurrency
    Crypto,
    /// stablecoin
    Stablecoin,
    /// fiat currency
    Fiat,
}

/// single L2 orderbook price level update
#[derive(Debug, Clone)]
pub struct L2Update {
    pub symbol: String,
    pub price: Decimal,
    pub volume: Decimal,
    pub side: Side,
    pub sequence: i64,
}

/// single price level within an L2 snapshot
#[derive(Debug, Clone)]
pub struct L2Level {
    pub price: Decimal,
    pub volume: Decimal,
}

/// full L2 orderbook snapshot
#[derive(Debug, Clone)]
pub struct L2Snapshot {
    pub symbol: String,
    // best ask (lowest) first, i.e. ascending
    pub asks: Vec<L2Level>,
    // best bid (higherst) first, i.e. descending
    pub bids: Vec<L2Level>,
    pub sequence: i64,
}

impl L2Snapshot {
    /// return true if both bid/ask are empty
    pub fn is_empty(&self) -> bool {
        self.asks.is_empty() && self.bids.is_empty()
    }

    /// Best (lowest) ask price, or `None` if the ask side is empty.                                   
    pub fn best_ask(&self) -> Option<L2Level> {
        self.asks.get(0).cloned()
    }

    /// Best (highest) bid price, or `None` if the bid side is empty.                                  
    pub fn best_bid(&self) -> Option<L2Level> {
        self.bids.get(0).cloned()
    }

    /// Bid-ask spread, or `None` if either side is empty.                                             
    pub fn spread(&self) -> Option<Decimal> {
        Some(self.best_ask()?.price - self.best_bid()?.price)
    }
}

/// forced liquidation event
#[derive(Debug, Clone)]
pub struct Liquidation {
    pub symbol: String,
    pub side: OrderSide,
    pub liquidated_user: String,
    pub notional_position: Decimal,
    pub account_value: Decimal,
}

/// snapshot of market metrics
#[derive(Debug, Clone)]
pub struct AssetContext {
    pub symbol: String,
    pub open_interest: Decimal,
    pub funding_rate: Decimal,
    pub mark_price: Decimal,
    pub day_volume: Decimal,
    pub mid_price: Option<Decimal>,
    pub oracle_price: Option<Decimal>,
    pub premium: Option<Decimal>,
    pub prev_day_price: Option<Decimal>,
    pub sz_decimals: i32,
}

/// predicted funding rate for a symbol at a venue
#[derive(Debug, Clone)]
pub struct PredictedFunding {
    pub symbol: String,
    pub venue: String,
    pub funding_rate: Decimal,
    pub next_funding_time_ms: i64,
}

/// market trade event
#[derive(Debug, Clone)]
pub struct Fill {
    pub symbol: String,
    pub price: Decimal,
    pub volume: Decimal,
    pub side: OrderSide,
    pub timestamp_ms: i64,
    pub trade_id: i64,
}

/// open trading position
#[derive(Debug, Clone)]
pub struct Position {
    pub symbol: String,
    pub side: OrderSide,
    pub size: Decimal,
    pub entry_price: Decimal,
}

/// resting order
#[derive(Debug, Clone)]
pub struct OpenOrder {
    pub order_id: i64,
    pub symbol: String,
    pub side: OrderSide,
    pub price: Decimal,
    pub quantity: Decimal,
    pub filled_quantity: Decimal,
    pub order_type: Option<OrderType>,
    pub trigger_price: Option<Decimal>,
    pub reduce_only: bool,
}

/// order placement response
#[derive(Debug, Clone)]
pub struct OrderPlacement {
    pub order_id: i64,
    pub symbol: String,
    pub side: OrderSide,
    pub price: Decimal,
    pub quantity: Decimal,
    pub timestamp_ms: i64,
    pub cloid: Option<String>,
    pub order_type: OrderType,
    pub trigger_price: Option<Decimal>,
    pub reduce_only: bool,
}

/// execution of the user's own order
#[derive(Debug, Clone)]
pub struct UserFill {
    pub order_id: i64,
    pub symbol: String,
    pub side: OrderSide,
    pub price: Decimal,
    pub quantity: Decimal,
    pub fee_usd: Decimal,
    pub timestamp_ms: i64,
    pub cloid: Option<String>,
}

/// order lifecycle update
#[derive(Debug, Clone)]
pub struct OrderUpdate {
    pub order_id: i64,
    pub symbol: String,
    pub status: OrderStatus,
    pub side: Option<OrderSide>,
    pub price: Option<Decimal>,
    pub quantity: Option<Decimal>,
    pub remaining_quantity: Option<Decimal>,
    pub timestamp_ms: i64,
    pub cloid: Option<String>,
}

/// funding payment applied to a position
#[derive(Debug, Clone)]
pub struct FundingPayment {
    pub symbol: String,
    pub amount_usd: Decimal,
    pub timestamp_ms: i64,
}

/// deposit event
#[derive(Debug, Clone)]
pub struct Deposit {
    pub asset: String,
    pub amount_usd: Decimal,
    pub timestamp_ms: i64,
}

/// withdrawal event
#[derive(Debug, Clone)]
pub struct Withdrawal {
    pub asset: String,
    pub amount_usd: Decimal,
    pub timestamp_ms: i64,
}

/// per-asset balance from spotClearinghouseState with margin health
#[derive(Debug, Clone)]
pub struct AccountBalance {
    pub token: String,
    pub equity: Decimal,
    pub free: Decimal,
    pub safe: Option<Decimal>,
    pub usable: Decimal,
    pub hold: Decimal,
    pub margin_used: Option<Decimal>,
    pub maintenance: Option<Decimal>,
}

/// user's address-level API rate limit budget from Hyperliquid's userRateLimit endpoint
#[derive(Debug, Clone)]
pub struct UserRateLimit {
    pub cumulative_volume: Decimal,
    pub requests_used: i64,
    pub requests_cap: i64,
    pub requests_surplus: i64,
}

/// test server network connection
#[allow(async_fn_in_trait)]
#[allow(clippy::too_many_arguments)]
pub trait TestServer {
    /// test ping
    async fn ping(&self) -> Result<bool, String>;
    /// get server local time
    async fn get_server_time(&self) -> Result<i64, String>;
}

/// get market data such as symbol, price and volume
#[allow(async_fn_in_trait)]
#[allow(clippy::too_many_arguments)]
pub trait GetMarketData {
    /// get symbol, such as BTCUSD
    async fn get_symbol(&self) -> Result<Vec<String>, String>;
    /// get mid-price of a symbol (e.g. BTCUSD -> 67000.0)
    async fn get_price(&self, symbol: String) -> Result<Decimal, String>;
    /// get current open interest for a symbol
    async fn get_open_interest(&self, symbol: String) -> Result<Decimal, String>;
    /// get snapshot of market metrics (OI, funding rate, mark price, 24h volume)
    async fn get_asset_context(&self, symbol: String) -> Result<AssetContext, String>;
    /// get all asset context snapshots in one call (prefer over repeated get_asset_context)
    async fn get_all_asset_contexts(&self) -> Result<Vec<AssetContext>, String>;
    /// get the number of decimal places for order size for a symbol (static metadata)
    async fn get_sz_decimals(&self, symbol: String) -> Result<i32, String>;
    /// get sz_decimals for all symbols in one call (prefer over repeated get_sz_decimals)
    async fn get_all_sz_decimals(&self) -> Result<HashMap<String, i32>, String>;
    /// get predicted funding rates for all symbols across all venues
    async fn get_predicted_fundings(&self) -> Result<Vec<PredictedFunding>, String>;
    /// get full L2 orderbook snapshot for a symbol (for initialization)
    async fn get_l2_orderbook(&self, symbol: String) -> Result<L2Snapshot, String>;
}

/// place, change, cancel order
#[allow(async_fn_in_trait)]
#[allow(clippy::too_many_arguments)]
pub trait ManageOrder {
    /// place order with optional client order ID for end-to-end tracking
    async fn place_order(
        &self,
        symbol: String,
        side: OrderSide,
        price: Decimal,
        volume: Decimal,
        order_type: OrderType,
        time_in_force: TimeInForce,
        trigger_price: Option<Decimal>,
        reduce_only: bool,
        cloid: Option<String>,
    ) -> Result<OrderPlacement, String>;
    /// change order
    async fn change_order_by_cloid(
        &self,
        cloid: i64,
        price: Decimal,
        volume: Decimal,
    ) -> Result<i64, String>;
    /// cancel order by cloid
    async fn cancel_order_by_cloid(&self, cloid: String) -> Result<(), String>;
    /// cancel all order regardless of cloid/symbol
    async fn cancel_all_order(&self) -> Result<bool, String>;
}

/// subscribe to streaming market data
#[allow(async_fn_in_trait)]
#[allow(clippy::too_many_arguments)]
pub trait SubscribeMarketData {
    /// subscribe to L2 orderbook updates for a symbol
    fn subscribe_l2_update(&self, symbol: String) -> BoxStream<Result<L2Update, String>>;
    /// subscribe to full L2 orderbook snapshots for a symbol
    fn subscribe_l2_snapshot(&self, symbol: String) -> BoxStream<Result<L2Snapshot, String>>;
    /// subscribe to market fill events for a symbol
    fn subscribe_fill(&self, symbol: String) -> BoxStream<Result<Fill, String>>;
    /// subscribe to asset context updates (OI, funding rate, mark price, 24h volume)
    fn subscribe_asset_context(&self, symbol: String) -> BoxStream<Result<AssetContext, String>>;
    /// subscribe to liquidation events for a user address
    fn subscribe_liquidation(&self, user: String) -> BoxStream<Result<Liquidation, String>>;
    /// unsubscribe from all market data streams (graceful shutdown)
    async fn unsubscribe_all(&self) -> ();
}

/// query authenticated account snapshot
#[allow(async_fn_in_trait)]
#[allow(clippy::too_many_arguments)]
pub trait GetAccountSnapshot {
    /// get current open positions
    async fn get_positions(&self) -> Result<Vec<Position>, String>;
    /// get currently resting orders
    async fn get_open_orders(&self) -> Result<Vec<OpenOrder>, String>;
    /// get all per-asset account balances with margin health
    async fn get_balance(&self) -> Result<Vec<AccountBalance>, String>;
    /// get user's address-level API rate limit budget (remaining requests, cumulative volume, cap)
    async fn get_user_rate_limit(&self) -> Result<UserRateLimit, String>;
}

/// subscribe to authenticated user account events
#[allow(async_fn_in_trait)]
#[allow(clippy::too_many_arguments)]
pub trait SubscribeUserEvents {
    /// stream executions of the user's own orders
    fn subscribe_user_fills(&self) -> BoxStream<Result<UserFill, String>>;
    /// stream order lifecycle updates
    fn subscribe_order_updates(&self) -> BoxStream<Result<OrderUpdate, String>>;
    /// stream funding payments applied to positions
    fn subscribe_funding_payments(&self) -> BoxStream<Result<FundingPayment, String>>;
    /// stream account deposit events
    fn subscribe_deposits(&self) -> BoxStream<Result<Deposit, String>>;
    /// stream account withdrawal events
    fn subscribe_withdrawals(&self) -> BoxStream<Result<Withdrawal, String>>;
    /// subscribe to spot wallet balance updates for the registered user address (requires authentication)
    fn subscribe_spot_balance(&self) -> BoxStream<Result<Vec<AccountBalance>, String>>;
    /// subscribe to spot wallet balance updates for a specific address
    fn subscribe_spot_balance_with_address(
        &self,
        address: String,
    ) -> BoxStream<Result<Vec<AccountBalance>, String>>;
    /// unsubscribe from all user event streams
    async fn unsubscribe_user_events(&self) -> ();
}

/// operational helpers for market data subscriptions (unsubscribe)
#[allow(async_fn_in_trait)]
#[allow(clippy::too_many_arguments)]
pub trait SubscribeMarketDataOps {
    /// unsubscribe from market data streams for a symbol
    async fn unsubscribe_market_data(&self, symbol: String) -> ();
}
