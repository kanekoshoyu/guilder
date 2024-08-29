/// orderbook model
pub mod orderbook;
/// currency pair model, e.g. BTC-USDT
pub mod currency_pair;

/// use macro to simplify below
pub use currency_pair::CurrencyPair;
pub use orderbook::{IndexOrderbook, Orderbook};
