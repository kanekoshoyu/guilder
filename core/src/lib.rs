/// orderbook data structures and live sync engine
#[cfg(feature = "engine")]
pub mod orderbook;

/// account state machine — deterministic event-sourced account tracking
#[cfg(feature = "account")]
pub mod account;

#[cfg(feature = "engine")]
pub use orderbook::{
    BookUpdate, CurrencyPair, EngineError, IndexOrderbook, Orderbook, OrderbookEngine,
    ReconciliationHealthView,
};

#[cfg(feature = "account")]
pub use account::{
    AccountEngine, AccountError, AccountEvent, AccountState, ExchangeSnapshot, OpenOrder,
    OrderStatus, Position, RecentFill, ReconciliationDiff, Side, SpotBalance,
};
