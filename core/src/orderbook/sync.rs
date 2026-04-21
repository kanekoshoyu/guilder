use std::sync::Arc;

use chrono::{DateTime, Utc};
use guilder_abstraction::{GetMarketData, SubscribeMarketData};
use std::collections::HashMap;
use std::sync::RwLock;
use tokio::sync::{broadcast, Semaphore};
use tokio_stream::StreamExt;
use tracing::{error, info, warn};

use super::convert::{apply_snapshot, apply_update, snapshot_to_book_updates};
use super::storage::PriceLevelStorage;
use super::types::{BookUpdate, Orderbook};

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
    info!(symbol = symbol, "orderbook REST snapshot requested");
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
    info!(
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
) where
    S: PriceLevelStorage + Default + Send + 'static,
    C: GetMarketData + SubscribeMarketData + Send + Sync + 'static,
{
    info!(
        symbol = symbol,
        ws_is_source_of_truth = ws_is_source_of_truth,
        "orderbook sync_loop started"
    );

    eprintln!(
        "[sync_loop] MEW sync_loop STARTED ws_is_source_of_truth={}",
        ws_is_source_of_truth
    );

    let mut msg_count: u64 = 0;

    loop {
        let mut update_stream = (!ws_is_source_of_truth).then(|| client.subscribe_l2_update(symbol.clone()));
        let mut snapshot_stream = ws_is_source_of_truth.then(|| client.subscribe_l2_snapshot(symbol.clone()));
        let mut last_seq: Option<i64> = None;
        loop {
            let timeout = tokio::time::sleep(std::time::Duration::from_secs(2));
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

                    let Some(result) = result else {
                        // Stream closed — break to outer reconnect.
                        warn!("stream is cloed");
                        break;
                    };
                    if symbol.eq("MEW") {
                        info!("[OB sync_loop] new MEW message received!");
                    }
                    match result {
                        StreamEvent::Snapshot(Ok(snapshot)) => {
                            let started = std::time::Instant::now();
                            if symbol == "MEW" {
                                warn!(
                                    symbol = symbol,
                                    sequence = snapshot.sequence,
                                    bid_levels = snapshot.bids.len(),
                                    ask_levels = snapshot.asks.len(),
                                    "orderbook snapshot received by sync_loop"
                                );
                            }
                            last_seq = Some(snapshot.sequence);
                            msg_count += 1;

                            let local_ts_ms = chrono::Utc::now().timestamp_millis() as u64;
                            let lag_ms = local_ts_ms.saturating_sub(snapshot.sequence as u64);
                            if symbol == "MEW" {
                                info!(
                                    exchange_ts = snapshot.sequence,
                                    local_ts = local_ts_ms,
                                    lag_ms = lag_ms,
                                    channel_len = update_tx.len(),
                                    "orderbook WS lag (MEW)"
                                );
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
                            if symbol == "MEW" {
                                warn!(
                                    symbol = symbol,
                                    sequence = snapshot.sequence,
                                    elapsed_ms = started.elapsed().as_millis(),
                                    update_fanout = snapshot.bids.len() + snapshot.asks.len(),
                                    "orderbook snapshot processed by sync_loop"
                                );
                            }
                        }
                        StreamEvent::Update(Ok(update)) => {
                            let new_message = last_seq.is_none_or(|prev| update.sequence != prev);

                            // hasnt recied any message ofter a while
                            if symbol.eq("MEW") {
                                info!("[OB sync_loop] new message for mew received from stream, {:#?}", update.sequence);
                            }
                            if new_message {
                                last_seq = Some(update.sequence);
                                msg_count += 1;

                                let local_ts_ms = chrono::Utc::now().timestamp_millis() as u64;
                                let lag_ms = local_ts_ms.saturating_sub(update.sequence as u64);
                                if symbol == "MEW" {
                                    info!(
                                        exchange_ts = update.sequence,
                                        local_ts = local_ts_ms,
                                        lag_ms = lag_ms,
                                        channel_len = update_tx.len(),
                                        "orderbook WS lag (MEW)"
                                    );
                                }
                                if symbol == "MEW" && (msg_count % 50 == 1 || msg_count <= 3) {
                                    eprintln!(
                                        "[sync_loop] MEW msg #{msg_count} seq={} ws_is_source_of_truth={}",
                                        update.sequence, ws_is_source_of_truth
                                    );
                                }

                                // When WS is the source of truth (e.g. Hyperliquid
                                // delivers full snapshots per tick), replace the book.

                                // this is true
                                if ws_is_source_of_truth {
                                    // create orderbook
                                    let mut book: Orderbook<S> = Orderbook::default();
                                    apply_update(&mut book, &update);
                                    // stop triggering after a while
                                    if symbol.eq("MEW") {
                                        info!("mew inserting");
                                    }
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
                            warn!(symbol = symbol, error = %e, "orderbook stream yielded error");
                            error!(symbol = symbol, error = %e, "orderbook WS stream error, reconnecting");
                            if !ws_is_source_of_truth {
                                // REST re-snapshot before resubscribing.
                                warn!(symbol = symbol, "REST resnapshot + resubscribe");
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
                            break;
                        }
                    }
                }
                _ = timeout => {
                    // No WS data for 2s — handle timeout here.
                    warn!(symbol = symbol, "orderbook WS heartbeat timeout (no data for 2s)");
                    // Optionally: force reconnect, log, etc.
                }
            }
        }

        // Stream ended (None) — reconnect after backoff.
        if symbol == "MEW" {
            eprintln!("[sync_loop] MEW stream ended after {msg_count} messages, reconnecting");
        }
        warn!(
            symbol = symbol,
            msg_count = msg_count,
            "orderbook WS stream ended, reconnecting"
        );
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }
}

enum StreamEvent {
    Snapshot(Result<guilder_abstraction::L2Snapshot, String>),
    Update(Result<guilder_abstraction::L2Update, String>),
}
