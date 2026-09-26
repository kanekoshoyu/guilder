pub mod client;
pub mod rate_limiter;
pub(crate) mod ws;
pub use client::*;
pub use client::HyperliquidNetwork;

/// Token string marking the PERP (futures) ledger row in `get_balance`
/// output. Under the manual-account model spot and futures are SEPARATE
/// ledgers; the futures row is appended LAST and carries `__PERP__` so
/// consumers can split the two without positional fragility.
pub const PERP_LEDGER_TOKEN: &str = "__PERP__";
