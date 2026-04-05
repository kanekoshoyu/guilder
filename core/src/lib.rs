/// data models: orderbook, currency pair
pub mod data;

/// orderbook engine: live sync + analytics
#[cfg(feature = "engine")]
pub mod engine;

pub use data::{CurrencyPair, IndexOrderbook, Orderbook};

#[cfg(feature = "engine")]
pub use engine::OrderbookEngine;
