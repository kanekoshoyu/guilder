use crate::status::{EngineStatus, StatusHandle};
use std::sync::Arc;

use chrono::{DateTime, Utc};
use guilder_abstraction::{GetMarketData, SubscribeMarketData};
use std::collections::HashMap;
use std::sync::RwLock;
use tokio::sync::{broadcast, Semaphore};
use tokio_stream::StreamExt;
use tracing::{debug, error, warn};

use super::convert::{apply_snapshot, apply_update, snapshot_to_book_updates};
use super::storage::PriceLevelStorage;
use super::types::{BookUpdate, Orderbook};

const LOOP_WATCHDOG_MS: u128 = 60_000;

/// Warn when waiting for the next stream item exceeds this multiple of the
/// symbol's OWN median inter-message gap — inactive symbols with sparse
/// updates are normal and must not spam the error buffer (2026-09-11: 234
/// symbols x ~5.4s gaps wall-papered /api/error every ~20s at the fixed 5s
/// threshold). Absolute floor keeps genuinely-dead streams (no watchdog from
/// LOOP_WATCHDOG_MS yet) visible in debug logs.
const WAIT_WARN_BASELINE_MULTIPLE: u128 = 4;
const WAIT_WARN_FLOOR_MS: u128 = 30_000;

/// Median of the most recent inter-message gaps (0 when no samples yet —
/// combined with the 30s floor this keeps early-connection logs quiet).
fn median_of(samples: &[u128]) -> u128 {
    if samples.is_empty() {
        return 0;
    }
    let mut v = samples.to_vec();
    v.sort_unstable();
    v[v.len() / 2]
}

/// Error marker sent by the WS manager during graceful shutdown.
/// Sync loops detect this and exit immediately without reconnecting.
const SHUTDOWN_MARKER: &str = "__orderbook_shutting_down__";

/// Check if an error indicates shutdown (either explicit marker or subscription closed).
fn is_shutdown_error(e: &str) -> bool {
    e == SHUTDOWN_MARKER || e == "websocket subscription closed"
}

fn is_transport_reset(error: &str) -> bool {
    error.contains("Connection reset")
        || error.contains("without closing handshake")
        || error.contains("Broken pipe")
        || error.contains("connection closed unexpectedly")
}

/// Re-snapshot a single symbol, acquiring the shared semaphore first to
/// prevent a thundering herd of REST calls when many streams gap at once.
async fn resnapshot<S, C>(
    client: &C,
    books: &RwLock<HashMap<String, Orderbook<S>>>,
    last_updated: &RwLock<HashMap<String, DateTime<Utc>>>,
    rest_semaphore: &Semaphore,
    symbol: &str,
) -> Option<i64>
where
    S: PriceLevelStorage + Default,
    C: GetMarketData,
{
    debug!(symbol = symbol, "orderbook REST snapshot requested");
    let start = std::time::Instant::now();
    let _permit = rest_semaphore.acquire().await.ok()?;
    let snapshot = client.get_l2_orderbook(symbol.to_owned()).await.ok()?;
    let seq = snapshot.sequence;
    let n_levels = snapshot.asks.len() + snapshot.bids.len();
    let mut book: Orderbook<S> = Orderbook::default();
    apply_snapshot(&mut book, &snapshot);
    books
        .write()
        .unwrap_or_else(|e| e.into_inner())
        .insert(symbol.to_owned(), book);
    last_updated
        .write()
        .unwrap_or_else(|e| e.into_inner())
        .insert(symbol.to_owned(), Utc::now());
    let elapsed = start.elapsed();
    debug!(
        symbol = symbol,
        levels = n_levels,
        exchange_ts = seq,
        elapsed_ms = elapsed.as_millis(),
        "orderbook REST snapshot complete"
    );
    Some(seq)
}

/// main function where the websocket messages are turned into storage
pub(crate) async fn sync_loop<S, C>(
    client: Arc<C>,
    books: Arc<RwLock<HashMap<String, Orderbook<S>>>>,
    last_updated: Arc<RwLock<HashMap<String, DateTime<Utc>>>>,
    update_tx: broadcast::Sender<BookUpdate>,
    rest_semaphore: Arc<Semaphore>,
    symbol: String,
    ws_is_source_of_truth: bool,
    status: StatusHandle,
) where
    S: PriceLevelStorage + Default + Send + 'static,
    C: GetMarketData + SubscribeMarketData + Send + Sync + 'static,
{
    let mut msg_count: u64 = 0;
    let mut reconnect_delay = std::time::Duration::from_secs(1);
    let mut shutting_down = false;

    loop {
        let mut update_stream =
            (!ws_is_source_of_truth).then(|| client.subscribe_l2_update(symbol.clone()));
        let mut snapshot_stream =
            ws_is_source_of_truth.then(|| client.subscribe_l2_snapshot(symbol.clone()));
        let mut last_seq: Option<i64> = None;
        let mut last_error: Option<String> = None;
        let mut last_event_at: Option<std::time::Instant> = None;
        let mut gap_samples: Vec<u128> = Vec::with_capacity(16);
        loop {
            let timeout = tokio::time::sleep(std::time::Duration::from_millis(
                LOOP_WATCHDOG_MS.try_into().unwrap(),
            ));
            let wait_started = std::time::Instant::now();
            tokio::select! {
                result = async {
                    if let Some(stream) = snapshot_stream.as_mut() {
                        stream.next().await.map(StreamEvent::Snapshot)
                    } else if let Some(stream) = update_stream.as_mut() {
                        stream.next().await.map(StreamEvent::Update)
                    } else {
                        None
                    }
                } => {
                    let wait_elapsed_ms = wait_started.elapsed().as_millis();
                    // Sample the inter-message gap (skip the first message of
                    // a fresh connection — no prior instant to diff against).
                    if let Some(prev) = last_event_at {
                        let gap = prev.elapsed().as_millis();
                        if gap_samples.len() == 16 {
                            gap_samples.remove(0);
                        }
                        gap_samples.push(gap);
                    }
                    last_event_at = Some(std::time::Instant::now());
                    let gap_median_ms = median_of(&gap_samples);
                    // Adaptive baseline: median of the symbol's own observed
                    // gaps (from recent wait samples). Sparse books are NORMAL.
                    if wait_elapsed_ms >= WAIT_WARN_FLOOR_MS
                        && wait_elapsed_ms >= gap_median_ms * WAIT_WARN_BASELINE_MULTIPLE
                        && status.get() == EngineStatus::Active
                    {
                        warn!(
                            symbol = symbol,
                            wait_elapsed_ms = wait_elapsed_ms,
                            gap_median_ms = gap_median_ms,
                            ws_is_source_of_truth = ws_is_source_of_truth,
                            "orderbook sync waited far longer than this symbol's own cadence"
                        );
                    }

                    let Some(result) = result else {
                        // Stream closed — break to outer reconnect.
                        warn!("stream is cloed");
                        break;
                    };
                    match result {
                        StreamEvent::Snapshot(Ok(snapshot)) => {
                            last_seq = Some(snapshot.sequence);
                            msg_count += 1;
                            if msg_count == 1 {
                                status.set(EngineStatus::Active);
                            }

                            let mut book: Orderbook<S> = Orderbook::default();
                            apply_snapshot(&mut book, &snapshot);
                            books
                                .write()
                                .unwrap_or_else(|e| e.into_inner())
                                .insert(symbol.clone(), book);
                            last_updated
                                .write()
                                .unwrap_or_else(|e| e.into_inner())
                                .insert(symbol.clone(), Utc::now());
                            for book_update in snapshot_to_book_updates(&snapshot) {
                                let _ = update_tx.send(book_update);
                            }
                        }
                        StreamEvent::Update(Ok(update)) => {
                            let new_message = last_seq.is_none_or(|prev| update.sequence != prev);
                            if new_message {
                                last_seq = Some(update.sequence);
                                msg_count += 1;
                                if msg_count == 1 {
                                    status.set(EngineStatus::Active);
                                }

                                // When WS is the source of truth (e.g. Hyperliquid
                                // delivers full snapshots per tick), replace the book.

                                // this is true
                                if ws_is_source_of_truth {
                                    // create orderbook
                                    let mut book: Orderbook<S> = Orderbook::default();
                                    apply_update(&mut book, &update);
                                    books
                                        .write()
                                        .unwrap_or_else(|e| e.into_inner())
                                        .insert(symbol.clone(), book);
                                } else {
                                    let mut books = books.write().unwrap_or_else(|e| e.into_inner());
                                    let book = books.entry(symbol.clone()).or_default();
                                    apply_update(book, &update);
                                }

                                last_updated
                                    .write()
                                    .unwrap_or_else(|e| e.into_inner())
                                    .insert(symbol.clone(), Utc::now());
                                let _ = update_tx.send(super::convert::to_book_update(&update));
                            } else {
                                // Additional level within the same WS message.
                                let mut books = books.write().unwrap_or_else(|e| e.into_inner());
                                let book = books.entry(symbol.clone()).or_default();
                                apply_update(book, &update);
                                let _ = update_tx.send(super::convert::to_book_update(&update));
                            }
                        }
                        StreamEvent::Snapshot(Err(e)) | StreamEvent::Update(Err(e)) => {
                            // Detect shutdown — exit outer loop immediately without warnings.
                            if is_shutdown_error(&e) {
                                shutting_down = true;
                                debug!(symbol = %symbol, "orderbook sync_loop received shutdown signal");
                                break;
                            }

                            let is_reset = is_transport_reset(&e);
                            if is_reset {
                                if !shutting_down {
                                    warn!(
                                        symbol = symbol,
                                        error = %e,
                                        "orderbook WS transport reset detected, reconnecting immediately"
                                    );
                                }
                                reconnect_delay = std::time::Duration::from_millis(50);
                            } else {
                                reconnect_delay = std::time::Duration::from_secs(1);
                                if status.get() == EngineStatus::Active && !shutting_down {
                                    warn!(symbol = symbol, error = %e, "orderbook stream yielded error");
                                    error!(symbol = symbol, error = %e, "orderbook WS stream error, reconnecting");
                                }
                            }
                            if !ws_is_source_of_truth {
                                // REST re-snapshot before resubscribing.
                                if status.get() == EngineStatus::Active && !shutting_down {
                                    warn!(symbol = symbol, "REST resnapshot + resubscribe");
                                }
                                let _ = resnapshot(
                                    client.as_ref(),
                                    &books,
                                    &last_updated,
                                    &rest_semaphore,
                                    &symbol,
                                )
                                .await;
                            }
                            // Break to outer loop — fresh stream after reconnect.
                            last_error = Some(e);
                            break;
                        }
                    }

                }
                _ = timeout => {
                    let wait_elapsed_ms = wait_started.elapsed().as_millis();
                    // No WS data for 5s — only warn once the symbol is Active.
                    // Suppress during Initializing (subscriptions warming up) and shutdown.
                    if status.get() == EngineStatus::Active && !shutting_down {
                        warn!(
                            symbol = symbol,
                            wait_elapsed_ms = wait_elapsed_ms,
                            ws_is_source_of_truth = ws_is_source_of_truth,
                            "orderbook WS heartbeat timeout (no data for 5s)"
                        );
                    }
                }
            }
        }

        if shutting_down {
            break;
        }

        if status.get() == EngineStatus::Active && !shutting_down {
            warn!(
                symbol = symbol,
                msg_count = msg_count,
                last_error = ?last_error,
                "orderbook WS stream ended, reconnecting"
            );
        }
        // Reset to Initializing so timeout stays suppressed during re-subscribe.
        status.set(EngineStatus::Initializing);
        msg_count = 0;
        tokio::time::sleep(reconnect_delay).await;
        reconnect_delay = std::time::Duration::from_secs(1);
    }
}

enum StreamEvent {
    Snapshot(Result<guilder_abstraction::L2Snapshot, String>),
    Update(Result<guilder_abstraction::L2Update, String>),
}
