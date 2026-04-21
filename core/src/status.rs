use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};

/// Lifecycle status of an engine or bridge.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineStatus {
    Initializing = 0,
    Active = 1,
    Disabled = 3,
    Disabling = 4,
}

impl EngineStatus {
    fn from_u8(v: u8) -> Self {
        match v {
            0 => Self::Initializing,
            1 => Self::Active,
            3 => Self::Disabled,
            4 => Self::Disabling,
            _ => Self::Disabled,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Initializing => "initializing",
            Self::Active => "active",
            Self::Disabled => "disabled",
            Self::Disabling => "disabling",
        }
    }
}

/// Cheaply cloneable handle to an engine's current status.
///
/// Writers call [`StatusHandle::set`]; readers call [`StatusHandle::get`].
/// Uses an `AtomicU8` so reads never block.
#[derive(Clone, Debug)]
pub struct StatusHandle(Arc<AtomicU8>);

impl StatusHandle {
    pub fn new(initial: EngineStatus) -> Self {
        Self(Arc::new(AtomicU8::new(initial as u8)))
    }

    pub fn set(&self, status: EngineStatus) {
        self.0.store(status as u8, Ordering::Relaxed);
    }

    pub fn get(&self) -> EngineStatus {
        EngineStatus::from_u8(self.0.load(Ordering::Relaxed))
    }
}
