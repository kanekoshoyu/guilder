use std::fmt;

/// Errors produced by the orderbook engine.
#[derive(Debug)]
pub enum EngineError {
    /// An exchange client call failed.
    Client(String),
    /// A symbol was not found in the engine.
    SymbolNotFound(String),
}

impl fmt::Display for EngineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EngineError::Client(msg) => write!(f, "client error: {msg}"),
            EngineError::SymbolNotFound(sym) => write!(f, "symbol not found: {sym}"),
        }
    }
}

impl std::error::Error for EngineError {}

impl From<String> for EngineError {
    fn from(s: String) -> Self {
        EngineError::Client(s)
    }
}
