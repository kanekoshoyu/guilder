use chrono::{DateTime, Utc};
use dashmap::DashMap;
use guilder_abstraction::{GetMarketData, Side, SubscribeMarketData};
use rust_decimal::Decimal;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tokio::sync::{broadcast, mpsc, Semaphore};
use tracing::{debug, info, warn};

use super::convert::apply_update;
use super::error::EngineError;
use super::storage::PriceLevelStorage;
use super::sync::sync_loop;
use super::types::{BookUpdate, Orderbook};

/// Per-symbol reconciliation health data.
#[derive(Debug, Clone)]
pub struct ReconciliationHealth {
    /// True if drift was detected during the last validation.
    pub drift_detected: bool,
    /// When the last REST validation completed.
    pub last_validation: Instant,
    /// Number of level mismatches found during last validation (0 if none).
    pub mismatch_levels: usize,
    /// Whether a correction was applied (book replaced with REST snapshot).
    pub corrected: bool,
}

/// Serialisable view of reconciliation health for API export.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ReconciliationHealthView {
    pub drift_detected: bool,
    pub last_validation_secs_ago: u64,
    pub mismatch_levels: usize,
    pub corrected: bool,
}

impl ReconciliationHealth {
    pub fn view(&self) -> ReconciliationHealthView {
        ReconciliationHealthView {
            drift_detected: self.drift_detected,
            last_validation_secs_ago: self.last_validation.elapsed().as_secs(),
            mismatch_levels: self.mismatch_levels,
            corrected: self.corrected,
        }
    }
}

impl Default for ReconciliationHealth {
    fn default() -> Self {
        Self {
            drift_detected: false,
            last_validation: Instant::now(),
            mismatch_levels: 0,
            corrected: false,
        }
    }
}

pub struct OrderbookEngine<C, S = std::collections::BTreeMap<Decimal, Decimal>>
where
    S: PriceLevelStorage + Default + Send + 'static,
{
    client: Arc<C>,
    books: Arc<DashMap<String, Orderbook<S>>>,
    last_updated: Arc<DashMap<String, DateTime<Utc>>>,
    add_tx: mpsc::UnboundedSender<Vec<String>>,
    add_rx: Mutex<Option<mpsc::UnboundedReceiver<Vec<String>>>>,
    update_tx: broadcast::Sender<BookUpdate>,
    /// Shared semaphore limiting concurrent REST calls across all sync loops.
    rest_semaphore: Arc<Semaphore>,
    /// When `true`, skip the initial REST snapshot and let the first WS message
    /// seed the orderbook. Useful for exchanges (e.g. Hyperliquid) whose WS
    /// streams deliver full snapshots on every tick.
    skip_initial_snapshot: bool,
    /// Interval between reconciliation checks. `None` = disabled.
    reconciliation_interval: Option<std::time::Duration>,
    /// Total number of drift detections (mismatches found).
    pub total_drifts: Arc<AtomicU64>,
    /// Total number of corrections applied (book replaced).
    pub total_corrections: Arc<AtomicU64>,
    /// Per-symbol reconciliation health — written by reconcile loop, read by handle.
    reconciliation_health: Arc<DashMap<String, ReconciliationHealth>>,
}

/// Maximum number of concurrent REST snapshot requests to avoid rate-limiting.
const SNAPSHOT_CONCURRENCY: usize = 10;

impl<C, S> OrderbookEngine<C, S>
where
    C: GetMarketData + SubscribeMarketData + Send + Sync + 'static,
    S: PriceLevelStorage + Default + Send + 'static,
{
    pub fn new(client: C) -> Self {
        let (add_tx, add_rx) = mpsc::unbounded_channel();
        let (update_tx, _) = broadcast::channel(4096);
        OrderbookEngine {
            client: Arc::new(client),
            books: Arc::new(DashMap::new()),
            last_updated: Arc::new(DashMap::new()),
            add_tx,
            add_rx: Mutex::new(Some(add_rx)),
            update_tx,
            rest_semaphore: Arc::new(Semaphore::new(SNAPSHOT_CONCURRENCY)),
            skip_initial_snapshot: false,
            reconciliation_interval: None,
            total_drifts: Arc::new(AtomicU64::new(0)),
            total_corrections: Arc::new(AtomicU64::new(0)),
            reconciliation_health: Arc::new(DashMap::new()),
        }
    }

    /// Skip the initial REST snapshot on `track()`/`track_all()`. The first WS
    /// message will seed each orderbook instead. Use this for exchanges that
    /// deliver full snapshots over WebSocket (e.g. Hyperliquid).
    pub fn with_skip_initial_snapshot(mut self, skip: bool) -> Self {
        self.skip_initial_snapshot = skip;
        self
    }

    /// Enable periodic reconciliation against REST snapshots.
    ///
    /// The reconciliation loop compares the local orderbook against a fresh
    /// REST fetch at the given interval. On mismatch, the local book is
    /// replaced with the REST snapshot. Drift counts and per-symbol health
    /// are exposed via [`OrderbookEngine::total_drifts`], [`OrderbookEngine::total_corrections`],
    /// and [`OrderbookEngine::reconciliation_health()`].
    pub fn with_reconciliation(mut self, interval: std::time::Duration) -> Self {
        self.reconciliation_interval = Some(interval);
        self
    }

    /// Set reconciliation interval on an already-constructed engine.
    /// Use with `Arc::get_mut()` before cloning or starting `track_all()`.
    pub fn set_reconciliation(&mut self, interval: std::time::Duration) {
        self.reconciliation_interval = Some(interval);
    }

    /// Subscribe to a broadcast channel of orderbook updates.
    /// Returns a receiver that yields `BookUpdate` on each price-level change.
    pub fn subscribe_updates(&self) -> broadcast::Receiver<BookUpdate> {
        self.update_tx.subscribe()
    }

    /// Subscribe to all symbols returned by `get_symbol()` and block, syncing
    /// orderbooks until all streams end. Caller should `tokio::task::spawn_local`
    /// or run this on a `LocalSet` since trait async fns may not be `Send`.
    pub async fn track_all(&self) -> Result<(), EngineError> {
        let symbols = self.client.get_symbol().await?;
        self.track(symbols).await
    }

    /// Spawn the reconciliation loop as a background task.
    /// Must be run inside a `tokio::task::LocalSet` (same constraint as `track_all`).
    /// Call this before `track()`/`track_all()` if you want reconciliation running.
    /// The task runs until the client's symbol stream ends or all symbols are dropped.
    pub fn spawn_reconciliation(self: &Arc<Self>) {
        let Some(interval) = self.reconciliation_interval else {
            return;
        };

        let books: Arc<DashMap<String, Orderbook<S>>> = Arc::clone(&self.books);
        let last_updated = Arc::clone(&self.last_updated);
        let client = Arc::clone(&self.client);
        let rest_semaphore = Arc::clone(&self.rest_semaphore);
        let total_drifts = Arc::clone(&self.total_drifts);
        let total_corrections = Arc::clone(&self.total_corrections);
        let health = Arc::clone(&self.reconciliation_health);

        tokio::task::spawn_local(async move {
            let mut ticker = tokio::time::interval(interval);
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                ticker.tick().await;

                // Collect snapshot of current symbols to reconcile.
                let symbols: Vec<String> = books.iter().map(|e| e.key().clone()).collect();
                if symbols.is_empty() {
                    continue;
                }

                for symbol in &symbols {
                    let rest_snapshot = match fetch_rest_book(client.as_ref(), &rest_semaphore, symbol).await
                    {
                        Some(s) => s,
                        None => {
                            health.insert(
                                symbol.clone(),
                                ReconciliationHealth {
                                    drift_detected: false,
                                    last_validation: Instant::now(),
                                    mismatch_levels: 0,
                                    corrected: false,
                                },
                            );
                            continue;
                        }
                    };

                    // Compare local vs REST.
                    let local = books.get(symbol);
                    let (mismatches, needs_replace) = match &local {
                        Some(entry) => {
                            compare_books(entry.value(), &rest_snapshot)
                        }
                        None => (0, true),
                    };

                    let corrected = if needs_replace {
                        let mut book: Orderbook<S> = Orderbook::default();
                        for update in &rest_snapshot {
                            apply_update(&mut book, update);
                        }
                        books.insert(symbol.clone(), book);
                        true
                    } else {
                        false
                    };

                    // Refresh last_updated on any successful REST validation
                    // (correction or clean match) — this prevents the staleness
                    // gate from rejecting low-activity coins that haven't seen
                    // a WS message recently.
                    last_updated.insert(symbol.clone(), Utc::now());

                    if mismatches > 0 {
                        total_drifts.fetch_add(1, Ordering::Relaxed);
                    }
                    if corrected {
                        total_corrections.fetch_add(1, Ordering::Relaxed);
                    }

                    health.insert(
                        symbol.clone(),
                        ReconciliationHealth {
                            drift_detected: mismatches > 0,
                            last_validation: Instant::now(),
                            mismatch_levels: mismatches,
                            corrected,
                        },
                    );
                }
            }
        });
    }

    /// Per-symbol reconciliation health (serialisable view).
    pub fn reconciliation_health(&self) -> Vec<(String, ReconciliationHealthView)> {
        self.reconciliation_health
            .iter()
            .map(|e| (e.key().clone(), e.value().view()))
            .collect()
    }

    /// Spawn a background staleness monitor that periodically checks all
    /// tracked orderbooks for:
    /// 1. **Wall-clock staleness** — no WS update received in `max_wall_age`
    /// 2. **Exchange timestamp skew** — exchange WS timestamp > `max_skew` behind wall clock
    ///
    /// Warns once per symbol per tick when thresholds are breached.
    /// Must be run inside a `tokio::task::LocalSet`.
    pub fn spawn_staleness_monitor(
        self: &Arc<Self>,
        max_wall_age: std::time::Duration,
        check_interval: std::time::Duration,
    ) {
        let last_updated = Arc::clone(&self.last_updated);

        tokio::task::spawn_local(async move {
            let mut ticker = tokio::time::interval(check_interval);
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                ticker.tick().await;

                let now = Utc::now();

                let entries: Vec<_> = last_updated
                    .iter()
                    .map(|e| (e.key().clone(), *e.value()))
                    .collect();

                if entries.is_empty() {
                    continue;
                }

                for (symbol, ts) in &entries {
                    let age = now.signed_duration_since(*ts);
                    if age.to_std().unwrap_or_default() > max_wall_age {
                        warn!(
                            symbol = symbol,
                            last_update_secs_ago = age.num_seconds(),
                            exchange_ts = ts.timestamp_millis(),
                            "orderbook stale — no update received"
                        );
                    }

                    debug!(
                        symbol = symbol,
                        wall_age_secs = age.num_seconds(),
                        exchange_ts = ts.timestamp_millis(),
                        "orderbook staleness check"
                    );
                }
            }
        });

        info!(
            max_wall_age_ms = max_wall_age.as_millis(),
            check_interval_ms = check_interval.as_millis(),
            "staleness monitor spawned"
        );
    }

    /// Subscribe to a specific set of symbols and block, syncing orderbooks.
    pub async fn track(&self, symbols: Vec<String>) -> Result<(), EngineError> {
        if !self.skip_initial_snapshot {
            self.snapshot_symbols(&symbols).await?;
        }

        let mut futures: Vec<_> = symbols
            .into_iter()
            .map(|symbol| {
                Box::pin(sync_loop(
                    Arc::clone(&self.client),
                    Arc::clone(&self.books),
                    Arc::clone(&self.last_updated),
                    self.update_tx.clone(),
                    Arc::clone(&self.rest_semaphore),
                    symbol,
                    self.skip_initial_snapshot,
                ))
            })
            .collect();

        // take the receiver — only one track() call drives the add-loop
        let mut add_rx = self.add_rx.lock().unwrap_or_else(|e| e.into_inner()).take();

        loop {
            if futures.is_empty() && add_rx.is_none() {
                break;
            }

            if futures.is_empty() {
                // only hot-add channel remains
                if let Some(rx) = &mut add_rx {
                    match rx.recv().await {
                        Some(new_symbols) => {
                            if !self.skip_initial_snapshot {
                                if let Err(e) = self.snapshot_symbols(&new_symbols).await {
                                    eprintln!("track_additional snapshot error: {e}");
                                    continue;
                                }
                            }
                            for symbol in new_symbols {
                                futures.push(Box::pin(sync_loop(
                                    Arc::clone(&self.client),
                                    Arc::clone(&self.books),
                                    Arc::clone(&self.last_updated),
                                    self.update_tx.clone(),
                                    Arc::clone(&self.rest_semaphore),
                                    symbol,
                                    self.skip_initial_snapshot,
                                )));
                            }
                        }
                        None => break, // channel closed
                    }
                } else {
                    break;
                }
                continue;
            }

            // race: either a sync_loop finishes, or new symbols arrive
            if let Some(rx) = &mut add_rx {
                tokio::select! {
                    result = futures::future::select_all(&mut futures) => {
                        let (_, idx, _) = result;
                        let _ = futures.remove(idx);
                    }
                    msg = rx.recv() => {
                        match msg {
                            Some(new_symbols) => {
                                if !self.skip_initial_snapshot {
                                    if let Err(e) = self.snapshot_symbols(&new_symbols).await {
                                        eprintln!("track_additional snapshot error: {e}");
                                        continue;
                                    }
                                }
                                for symbol in new_symbols {
                                    futures.push(Box::pin(sync_loop(
                                        Arc::clone(&self.client),
                                        Arc::clone(&self.books),
                                        Arc::clone(&self.last_updated),
                                        self.update_tx.clone(),
                                        Arc::clone(&self.rest_semaphore),
                                        symbol,
                                        self.skip_initial_snapshot,
                                    )));
                                }
                            }
                            None => { add_rx = None; }
                        }
                    }
                }
            } else {
                let (_, _, remaining) = futures::future::select_all(futures).await;
                futures = remaining;
            }
        }

        Ok(())
    }

    /// Non-blocking: add symbols to a running `track()` loop.
    /// The engine will snapshot and start sync loops for them in the background.
    pub fn track_additional(&self, symbols: Vec<String>) -> Result<(), EngineError> {
        self.add_tx
            .send(symbols)
            .map_err(|e| EngineError::Client(format!("track loop not running: {e}")))
    }

    /// Snapshot and insert initial orderbooks for a batch of symbols.
    /// Limits concurrency to [`Self::SNAPSHOT_CONCURRENCY`] to stay within REST budgets.
    async fn snapshot_symbols(&self, symbols: &[String]) -> Result<(), EngineError> {
        use futures::stream::{self, StreamExt};

        let results: Vec<_> = stream::iter(symbols.iter().cloned())
            .map(|sym| {
                let client = Arc::clone(&self.client);
                async move {
                    let snapshot = client.get_l2_orderbook(sym.clone()).await?;
                    Ok::<_, String>((sym, snapshot))
                }
            })
            .buffer_unordered(SNAPSHOT_CONCURRENCY)
            .collect()
            .await;

        for result in results {
            let (sym, snapshot) = result?;
            let mut book: Orderbook<S> = Orderbook::default();
            for update in &snapshot {
                apply_update(&mut book, update);
            }
            self.books.insert(sym.clone(), book);
            self.last_updated.insert(sym, Utc::now());
        }

        Ok(())
    }

    /// Returns the top `depth` levels per side for a symbol.
    /// If `depth` is `None`, returns all levels.
    pub fn snapshot(&self, symbol: &str, depth: Option<usize>) -> Option<Vec<(Side, Decimal, Decimal)>> {
        let book = self.books.get(symbol)?;
        Some(book.snapshot(depth))
    }

    /// Last exchange timestamp for this symbol's orderbook.
    pub fn last_updated(&self, symbol: &str) -> Option<DateTime<Utc>> {
        self.last_updated.get(symbol).map(|r| *r.value())
    }

    /// Health status for all tracked symbols: `(symbol, last_exchange_timestamp)`.
    pub fn health(&self) -> Vec<(String, DateTime<Utc>)> {
        self.last_updated
            .iter()
            .map(|entry| (entry.key().clone(), *entry.value()))
            .collect()
    }

    /// Quote-currency liquidity within a slippage boundary on one side.
    pub fn liquidity(&self, symbol: &str, side: Side, slippage_pct: f64) -> Option<Decimal> {
        let book = self.books.get(symbol)?;
        book.liquidity(side, slippage_pct)
    }

    /// Liquidity imbalance ratio `(B - A) / (B + A)`.
    /// `top_n` limits to top N levels per side; `None` uses the full book.
    pub fn imbalance(&self, symbol: &str, top_n: Option<usize>) -> Option<f64> {
        let book = self.books.get(symbol)?;
        book.imbalance(top_n)
    }
}

// ---------------------------------------------------------------------------
// Reconciliation helpers — standalone functions for the spawned task.
// ---------------------------------------------------------------------------

/// Fetch a REST snapshot and return it as a flat list of updates.
/// Returns `None` on failure.
async fn fetch_rest_book<C>(
    client: &C,
    rest_semaphore: &Semaphore,
    symbol: &str,
) -> Option<Vec<guilder_abstraction::L2Update>>
where
    C: GetMarketData,
{
    let _permit = rest_semaphore.acquire().await.ok()?;
    client.get_l2_orderbook(symbol.to_owned()).await.ok()
}

/// Compare a local orderbook against a REST snapshot.
/// Returns `(mismatch_count, needs_replace)`.
/// `needs_replace` is true when any level differs — even a single mismatch
/// means the local book is stale, since Hyperliquid WS sends the full book
/// every tick, not incremental diffs.
fn compare_books<S: PriceLevelStorage>(
    local: &Orderbook<S>,
    rest_snapshot: &[guilder_abstraction::L2Update],
) -> (usize, bool) {
    let local_levels = local.snapshot(None);

    // Build sorted vectors from both sides for comparison.
    // Key by (side_as_int, price) so we can use Vec + sort instead of HashMap.
    let mut local_sorted: Vec<(u8, Decimal, Decimal)> = local_levels
        .iter()
        .map(|(side, price, vol)| {
            let side_tag = match side {
                Side::Bid => 0u8,
                Side::Ask => 1u8,
            };
            (side_tag, *price, *vol)
        })
        .collect();
    local_sorted.sort_by_key(|(s, p, _)| (*s, *p));

    let mut rest_sorted: Vec<(u8, Decimal, Decimal)> = rest_snapshot
        .iter()
        .map(|u| {
            let side_tag = match u.side {
                Side::Bid => 0u8,
                Side::Ask => 1u8,
            };
            (side_tag, u.price, u.volume)
        })
        .collect();
    rest_sorted.sort_by_key(|(s, p, _)| (*s, *p));

    // Two-pointer diff to count mismatches.
    let mut mismatches = 0usize;
    let mut i = 0;
    let mut j = 0;
    while i < local_sorted.len() && j < rest_sorted.len() {
        let (ls, lp, lv) = &local_sorted[i];
        let (rs, rp, rv) = &rest_sorted[j];
        match (ls, lp).cmp(&(rs, rp)) {
            std::cmp::Ordering::Equal => {
                if *lv != *rv {
                    mismatches += 1;
                }
                i += 1;
                j += 1;
            }
            std::cmp::Ordering::Less => {
                // Level in local but not in REST
                mismatches += 1;
                i += 1;
            }
            std::cmp::Ordering::Greater => {
                // Level in REST but not in local
                mismatches += 1;
                j += 1;
            }
        }
    }
    // Remaining levels on either side are mismatches
    mismatches += local_sorted.len() - i;
    mismatches += rest_sorted.len() - j;

    let needs_replace = mismatches > 0;
    (mismatches, needs_replace)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    fn make_update(symbol: &str, price: Decimal, volume: Decimal, side: Side) -> guilder_abstraction::L2Update {
        guilder_abstraction::L2Update {
            symbol: symbol.to_string(),
            price,
            volume,
            side,
            sequence: 0,
        }
    }

    #[test]
    fn test_compare_books_identical() {
        let mut book = Orderbook::new();
        book.update_bid(dec!(100), dec!(10));
        book.update_ask(dec!(101), dec!(5));

        let rest = vec![
            make_update("BTC", dec!(101), dec!(5), Side::Ask),
            make_update("BTC", dec!(100), dec!(10), Side::Bid),
        ];

        let (mismatches, needs_replace) = compare_books(&book, &rest);
        assert_eq!(mismatches, 0);
        assert!(!needs_replace);
    }

    #[test]
    fn test_compare_books_volume_drift() {
        let mut book = Orderbook::new();
        book.update_bid(dec!(100), dec!(10));
        book.update_ask(dec!(101), dec!(5));

        let rest = vec![
            make_update("BTC", dec!(101), dec!(5), Side::Ask),
            make_update("BTC", dec!(100), dec!(8), Side::Bid), // volume changed
        ];

        let (mismatches, needs_replace) = compare_books(&book, &rest);
        assert_eq!(mismatches, 1);
        assert!(needs_replace);
    }

    #[test]
    fn test_compare_books_missing_level() {
        let mut book = Orderbook::new();
        book.update_bid(dec!(100), dec!(10));
        book.update_bid(dec!(99), dec!(5));
        book.update_ask(dec!(101), dec!(5));

        let rest = vec![
            make_update("BTC", dec!(101), dec!(5), Side::Ask),
            make_update("BTC", dec!(100), dec!(10), Side::Bid),
            // level at 99 removed
        ];

        let (mismatches, needs_replace) = compare_books(&book, &rest);
        assert_eq!(mismatches, 1);
        assert!(needs_replace);
    }

    #[test]
    fn test_compare_books_new_level() {
        let mut book = Orderbook::new();
        book.update_bid(dec!(100), dec!(10));
        book.update_ask(dec!(101), dec!(5));

        let rest = vec![
            make_update("BTC", dec!(101), dec!(5), Side::Ask),
            make_update("BTC", dec!(100), dec!(10), Side::Bid),
            make_update("BTC", dec!(99), dec!(3), Side::Bid), // new level
        ];

        let (mismatches, needs_replace) = compare_books(&book, &rest);
        assert_eq!(mismatches, 1);
        assert!(needs_replace);
    }

    #[test]
    fn test_compare_books_empty_local() {
        let book = Orderbook::new();
        let rest = vec![
            make_update("BTC", dec!(101), dec!(5), Side::Ask),
            make_update("BTC", dec!(100), dec!(10), Side::Bid),
        ];

        let (mismatches, needs_replace) = compare_books(&book, &rest);
        assert_eq!(mismatches, 2);
        assert!(needs_replace);
    }

    #[test]
    fn test_compare_books_both_empty() {
        let book = Orderbook::new();
        let rest: Vec<guilder_abstraction::L2Update> = vec![];

        let (mismatches, needs_replace) = compare_books(&book, &rest);
        assert_eq!(mismatches, 0);
        assert!(!needs_replace);
    }
}
