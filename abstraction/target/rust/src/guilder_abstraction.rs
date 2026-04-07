use rust_decimal::Decimal;
use std::pin::Pin;
use futures_core::Stream;

pub type BoxStream<T> = Pin<Box<dyn Stream<Item = T> + Send + 'static>>;

#[derive(Debug, Clone, PartialEq)]
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
#[derive(Debug, Clone, PartialEq)]
pub enum Side {
	/// bid side
	Bid,
	/// ask side
	Ask,
}

/// direction of an order
#[derive(Debug, Clone, PartialEq)]
pub enum OrderSide {
	/// buy order
	Buy,
	/// sell order
	Sell,
}

/// lifecycle state of an order
#[derive(Debug, Clone, PartialEq)]
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
#[derive(Debug, Clone, PartialEq)]
pub enum MarketType {
	/// spot market
	Spot,
	/// dated futures
	Future,
	/// perpetual futures
	Perpetual,
}

/// order execution type
#[derive(Debug, Clone, PartialEq)]
pub enum OrderType {
	/// execute immediately
	Market,
	/// execute at specified price
	Limit,
}

/// how long an order remains active
#[derive(Debug, Clone, PartialEq)]
pub enum TimeInForce {
	/// good till cancel
	Gtc,
	/// immediate or cancel
	Ioc,
	/// fill or kill
	Fok,
}

/// which currency the volume is expressed in
#[derive(Debug, Clone, PartialEq)]
pub enum VolumeDenomination {
	/// base currency
	Base,
	/// quote currency
	Quote,
}

/// broad class of asset
#[derive(Debug, Clone, PartialEq)]
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

/// account balance for an asset
#[derive(Debug, Clone)]
pub struct Balance {
	pub coin: String,
	pub total: Decimal,
	pub available: Decimal,
	pub locked: Decimal,
}

/// test server network connection
#[allow(async_fn_in_trait)]
pub trait TestServer {
	/// test ping
	async fn ping(&self) -> Result<bool, String>;
	/// get server local time
	async fn get_server_time(&self) -> Result<i64, String>;
}

/// get market data such as symbol, price and volume
#[allow(async_fn_in_trait)]
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
	/// get predicted funding rates for all symbols across all venues
	async fn get_predicted_fundings(&self) -> Result<Vec<PredictedFunding>, String>;
	/// get full L2 orderbook snapshot for a symbol (for initialization)
	async fn get_l2_orderbook(&self, symbol: String) -> Result<Vec<L2Update>, String>;
}

/// place, change, cancel order
#[allow(async_fn_in_trait)]
pub trait ManageOrder {
	/// place order with optional client order ID for end-to-end tracking
	async fn place_order(&self, symbol: String, side: OrderSide, price: Decimal, volume: Decimal, order_type: OrderType, time_in_force: TimeInForce, cloid: Option<String>) -> Result<OrderPlacement, String>;
	/// change order
	async fn change_order_by_cloid(&self, cloid: i64, price: Decimal, volume: Decimal) -> Result<i64, String>;
	/// cancel order by cloid
	async fn cancel_order(&self, cloid: i64) -> Result<i64, String>;
	/// cancel all order regardless of cloid/symbol
	async fn cancel_all_order(&self) -> Result<bool, String>;
}

/// subscribe to streaming market data
#[allow(async_fn_in_trait)]
pub trait SubscribeMarketData {
	/// subscribe to L2 orderbook updates for a symbol
	fn subscribe_l2_update(&self, symbol: String) -> BoxStream<Result<L2Update, String>>;
	/// subscribe to market fill events for a symbol
	fn subscribe_fill(&self, symbol: String) -> BoxStream<Result<Fill, String>>;
	/// subscribe to asset context updates (OI, funding rate, mark price, 24h volume)
	fn subscribe_asset_context(&self, symbol: String) -> BoxStream<Result<AssetContext, String>>;
	/// subscribe to liquidation events for a user address
	fn subscribe_liquidation(&self, user: String) -> BoxStream<Result<Liquidation, String>>;
}

/// query authenticated account snapshot
#[allow(async_fn_in_trait)]
pub trait GetAccountSnapshot {
	/// get current open positions
	async fn get_positions(&self) -> Result<Vec<Position>, String>;
	/// get currently resting orders
	async fn get_open_orders(&self) -> Result<Vec<OpenOrder>, String>;
	/// get available account collateral
	async fn get_collateral(&self) -> Result<Decimal, String>;
	/// get all spot wallet balances
	async fn get_spot_balance(&self) -> Result<Vec<Balance>, String>;
	/// get clearing house collateral balance (typically USDC only)
	async fn get_collateral_balance(&self, asset: String) -> Result<Balance, String>;
}

/// subscribe to authenticated user account events
#[allow(async_fn_in_trait)]
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
	fn subscribe_spot_balance(&self) -> BoxStream<Result<Vec<Balance>, String>>;
	/// subscribe to spot wallet balance updates for a specific address
	fn subscribe_spot_balance_with_address(&self, address: String) -> BoxStream<Result<Vec<Balance>, String>>;
}

