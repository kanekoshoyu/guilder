use std::sync::Arc;

use arc_swap::ArcSwap;
use tokio::sync::mpsc;
use tracing::warn;

use super::event::AccountEvent;
use super::types::AccountState;

/// Holds the authoritative in-memory [`AccountState`] behind an `ArcSwap`
/// so readers can get lock-free point-in-time snapshots.
///
/// All mutations go through [`AccountEngine::process`], which clones the
/// current state, applies the event, and atomically swaps in the new copy.
/// Each processed event is also broadcast on the channel.
pub struct AccountEngine {
    state: Arc<ArcSwap<AccountState>>,
    tx: mpsc::UnboundedSender<AccountEvent>,
}

impl AccountEngine {
    /// Creates a new engine and returns the receiver half.
    pub fn new() -> (
        Self,
        mpsc::UnboundedReceiver<AccountEvent>,
        Arc<ArcSwap<AccountState>>,
    ) {
        let (tx, rx) = mpsc::unbounded_channel();
        let state = Arc::new(ArcSwap::from_pointee(AccountState::default()));
        (
            Self {
                state: Arc::clone(&state),
                tx,
            },
            rx,
            state,
        )
    }

    /// Lock-free point-in-time snapshot of current account state.
    pub fn state(&self) -> Arc<AccountState> {
        self.state.load_full()
    }

    /// Handle suitable for sharing with consumers who only need read access.
    pub fn state_handle(&self) -> Arc<ArcSwap<AccountState>> {
        Arc::clone(&self.state)
    }

    /// Apply `event` to the in-memory state and broadcast it.
    ///
    /// Errors from [`AccountState::apply`] are logged and the event is
    /// dropped (not forwarded) so downstream consumers only see
    /// consistent transitions.
    pub fn process(&mut self, event: AccountEvent) {
        let mut state = self.state.load().as_ref().clone();
        if let Err(e) = state.apply(&event) {
            warn!("AccountState::apply error: {e}");
            return;
        }
        self.state.store(Arc::new(state));
        let _ = self.tx.send(event);
    }
}
