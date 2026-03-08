use futures_core::Stream;

#[derive(Debug, Clone, PartialEq)]
pub enum Status {
	/// The task is pending.
	Success,
	/// The task is currently in progress.
	InProgress,
	/// The task has been completed.
	Completed,
	/// The task has failed.
	Failed,
}

/// orderbook side
#[derive(Debug, Clone, PartialEq)]
pub enum Side {
	/// bid side of the orderbook
	Bid,
	/// ask side of the orderbook
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

/// type of market
#[derive(Debug, Clone, PartialEq)]
pub enum MarketType {
	/// spot market
	Spot,
	/// dated futures contract
	Future,
	/// perpetual futures contract
	Perpetual,
}

/// order execution type
#[derive(Debug, Clone, PartialEq)]
pub enum OrderType {
	/// execute immediately at best available price
	Market,
	/// execute at specified price or better
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
	/// volume in base currency (e.g. BTC in BTC-USDT)
	Base,
	/// volume in quote currency (e.g. USDT in BTC-USDT)
	Quote,
}

/// broad class of an asset
#[derive(Debug, Clone, PartialEq)]
pub enum AssetClass {
	/// non-stable cryptocurrency
	Crypto,
	/// price-stable cryptocurrency
	Stablecoin,
	/// government-issued currency
	Fiat,
}

/// single L2 orderbook price level update
#[derive(Debug, Clone)]
pub struct L2Update {
	pub symbol: String,
	pub price: f64,
	pub volume: f64,
	pub side: Side,
	pub sequence: i64,
}

/// forced liquidation event for a user
#[derive(Debug, Clone)]
pub struct Liquidation {
	pub liquidated_user: String,
	pub notional_position: f64,
	pub account_value: f64,
}

/// snapshot of key market metrics for an asset
#[derive(Debug, Clone)]
pub struct AssetContext {
	pub symbol: String,
	pub open_interest: f64,
	pub funding_rate: f64,
	pub mark_price: f64,
	pub day_volume: f64,
}

/// market trade/fill event
#[derive(Debug, Clone)]
pub struct Fill {
	pub symbol: String,
	pub price: f64,
	pub volume: f64,
	pub side: OrderSide,
	pub timestamp_ms: i64,
	pub trade_id: i64,
}

/// test server network connection
#[allow(async_fn_in_trait)]
pub trait TestServer {
	/// test ping
	async fn ping(&self) -> bool;
	/// get server local time
	async fn get_server_time(&self) -> i64;
}

/// get market data such as symbol, price and volume
#[allow(async_fn_in_trait)]
pub trait GetMarketData {
	/// get symbol, such as BTCUSD
	async fn get_symbol(&self) -> Vec<String>;
	/// get mid-price of a symbol (e.g. BTCUSD -> 67000.0)
	async fn get_price(&self, symbol: String) -> f64;
	/// get current open interest for a symbol
	async fn get_open_interest(&self, symbol: String) -> f64;
}

/// place, change, cancel order
#[allow(async_fn_in_trait)]
pub trait ManageOrder {
	/// place order, return cloid
	async fn place_order(&self, symbol: String, price: f64, volume: f64) -> i64;
	/// change order
	async fn change_order_by_cloid(&self, cloid: i64, price: f64, volume: f64) -> i64;
	/// cancel order by cloid
	async fn cancel_order(&self, cloid: i64) -> i64;
	/// cancel all order regardless of cloid/symbol
	async fn cancel_all_order(&self) -> bool;
}

/// subscribe to streaming market data
#[allow(async_fn_in_trait)]
pub trait SubscribeMarketData {
	/// subscribe to L2 orderbook updates for a symbol
	fn subscribe_l2_update(&self, symbol: String) -> impl Stream<Item = L2Update>;
	/// subscribe to market fill events for a symbol
	fn subscribe_fill(&self, symbol: String) -> impl Stream<Item = Fill>;
	/// subscribe to asset context updates (OI, funding rate, mark price, 24h volume)
	fn subscribe_asset_context(&self, symbol: String) -> impl Stream<Item = AssetContext>;
	/// subscribe to liquidation events for a user address
	fn subscribe_liquidation(&self, user: String) -> impl Stream<Item = Liquidation>;
}

