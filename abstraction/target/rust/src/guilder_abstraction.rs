use std::collections::HashMap;

/// test server network connection
pub trait TestServer {
	/// test ping
	fn ping(&self) -> bool;
	/// get server local time
	fn get_server_time(&self) -> i64;
}

/// get market data such as symbol, price and volume
pub trait GetMarketData {
	/// get symbol, such as BTCUSD
	fn get_symbol(&self) -> Vec<String>;
	/// get mid-price of a symbol (e.g. BTCUSD -> 67000.0)
	fn get_price(&self, symbol: String) -> f64;
	/// get orderbook
	fn get_orderbook(&self, symbol: String) -> HashMap<f64, f64>;
}

/// place, change, cancel order
pub trait ManageOrder {
	/// place order, return cloid
	fn place_order(&self, symbol: String, price: i32, volume: i32) -> i64;
	/// change order
	fn change_order_by_cloid(&self, cloid: i64, price: i32, volume: i32) -> i64;
	/// cancel order by cloid
	fn cancel_order(&self, cloid: i64) -> i64;
	/// cancel all order regardless of cloid/symbol
	fn cancel_all_order(&self) -> bool;
}

