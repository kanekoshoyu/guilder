/// orderbook data structures and live sync engine
#[cfg(feature = "engine")]
pub mod orderbook;

/// account state machine — deterministic event-sourced account tracking
#[cfg(feature = "account")]
pub mod account;

#[cfg(feature = "engine")]
pub use orderbook::{
    BookUpdate, BoundedVec, CurrencyPair, EngineError, Orderbook, OrderbookEngine,
    ReconciliationHealthView, SortedVec,
};

#[cfg(feature = "account")]
pub use account::{
    AccountEngine, AccountError, AccountEvent, AccountState, ExchangeSnapshot, OpenOrder,
    OrderStatus, Position, RecentFill, ReconciliationDiff, Side, SpotBalance,
};
