use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};

/// Lifecycle status of an engine or bridge.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum EngineStatus {
    Initializing = 0,
    Active = 1,
    Disabled = 3,
    Disabling = 4,
}

impl std::fmt::Display for EngineStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
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
#[cfg_attr(not(feature = "tracing"), allow(dead_code))]
#[derive(Debug)]
struct StatusInner {
    state: AtomicU8,
    label: Option<Arc<str>>,
    trace_transitions: bool,
}

#[derive(Clone, Debug)]
pub struct StatusHandle(Arc<StatusInner>);

impl StatusHandle {
    pub fn new(initial: EngineStatus) -> Self {
        Self(Arc::new(StatusInner {
            state: AtomicU8::new(initial as u8),
            label: None,
            trace_transitions: false,
        }))
    }

    pub fn named(
        initial: EngineStatus,
        label: impl Into<Option<String>>,
    ) -> Self {
        Self(Arc::new(StatusInner {
            state: AtomicU8::new(initial as u8),
            label: label.into().map(Arc::<str>::from),
            trace_transitions: true,
        }))
    }

    pub fn set(&self, status: EngineStatus) {
        let previous = self.get();
        if previous == status {
            return;
        }

        self.0.state.store(status as u8, Ordering::Relaxed);

        #[cfg(feature = "tracing")]
        if self.0.trace_transitions {
            if let Some(label) = &self.0.label {
                tracing::info!(
                    label = %label,
                    from = previous.as_str(),
                    to = status.as_str(),
                    "engine status transitioned"
                );
            } else {
                tracing::info!(
                    from = previous.as_str(),
                    to = status.as_str(),
                    "engine status transitioned"
                );
            }
        }
    }

    pub fn get(&self) -> EngineStatus {
        EngineStatus::from_u8(self.0.state.load(Ordering::Relaxed))
    }
}
