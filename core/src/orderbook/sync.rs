use std::sync::Arc;

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use guilder_abstraction::{GetMarketData, SubscribeMarketData};
use tokio::sync::{broadcast, Semaphore};
use tokio_stream::StreamExt;
use tracing::{error, info, warn};

use super::convert::apply_update;
use super::storage::PriceLevelStorage;
use super::types::{BookUpdate, Orderbook};

/// Re-snapshot a single symbol, acquiring the shared semaphore first to
/// prevent a thundering herd of REST calls when many streams gap at once.
async fn resnapshot<S, C>(
    client: &C,
    books: &DashMap<String, Orderbook<S>>,
    last_updated: &DashMap<String, DateTime<Utc>>,
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
    let seq = snapshot.last().map(|u| u.sequence);
    let n_levels = snapshot.len();
    let mut book: Orderbook<S> = Orderbook::default();
    for u in &snapshot {
        apply_update(&mut book, u);
    }
    books.insert(symbol.to_owned(), book);
    last_updated.insert(symbol.to_owned(), Utc::now());
    let elapsed = start.elapsed();
    info!(
        symbol = symbol,
        levels = n_levels,
        exchange_ts = seq,
        elapsed_ms = elapsed.as_millis(),
        "orderbook REST snapshot complete"
    );
    seq
}

/// main function where the websocket messages are turned into storage
pub(crate) async fn sync_loop<S, C>(
    client: Arc<C>,
    books: Arc<DashMap<String, Orderbook<S>>>,
    last_updated: Arc<DashMap<String, DateTime<Utc>>>,
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
        let mut stream = client.subscribe_l2_update(symbol.clone());
        let mut last_seq: Option<i64> = None;
        loop {
            let timeout = tokio::time::sleep(std::time::Duration::from_secs(2));
            tokio::select! {
                result = stream.next() => {

                    let Some(result) = result else {
                        // Stream closed — break to outer reconnect.
                        warn!("stream is cloed");
                        break;
                    };
                    if symbol.eq("MEW") {
                        info!("[OB sync_loop] new MEW message received!");
                    }
                    match result {
                        Ok(update) => {
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
                                    books.insert(symbol.clone(), book);
                                } else {
                                    let mut book = books.entry(symbol.clone()).or_default();
                                    apply_update(book.value_mut(), &update);
                                }

                                last_updated.insert(symbol.clone(), Utc::now());
                                let _ = update_tx.send(super::convert::to_book_update(&update));
                            } else {
                                // Additional level within the same WS message.
                                let mut book = books.entry(symbol.clone()).or_default();
                                apply_update(book.value_mut(), &update);
                                let _ = update_tx.send(super::convert::to_book_update(&update));
                            }
                        }
                        Err(e) => {
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
