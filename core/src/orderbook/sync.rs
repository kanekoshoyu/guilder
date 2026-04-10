use std::sync::Arc;

use dashmap::DashMap;
use guilder_abstraction::{GetMarketData, SubscribeMarketData};
use tokio::sync::{broadcast, Semaphore};
use tokio::time::Instant;
use tokio_stream::StreamExt;

use super::convert::{apply_update, to_book_update};
use super::types::{BookUpdate, Orderbook};

/// Re-snapshot a single symbol, acquiring the shared semaphore first to
/// prevent a thundering herd of REST calls when many streams gap at once.
async fn resnapshot<C>(
    client: &C,
    books: &DashMap<String, Orderbook>,
    last_updated: &DashMap<String, Instant>,
    rest_semaphore: &Semaphore,
    symbol: &str,
) -> Option<i64>
where
    C: GetMarketData,
{
    let _permit = rest_semaphore.acquire().await.ok()?;
    let snapshot = client.get_l2_orderbook(symbol.to_owned()).await.ok()?;
    let seq = snapshot.last().map(|u| u.sequence);
    let mut book = Orderbook::new();
    for u in &snapshot {
        apply_update(&mut book, u);
    }
    books.insert(symbol.to_owned(), book);
    last_updated.insert(symbol.to_owned(), Instant::now());
    seq
}

pub(crate) async fn sync_loop<C>(
    client: Arc<C>,
    books: Arc<DashMap<String, Orderbook>>,
    last_updated: Arc<DashMap<String, Instant>>,
    update_tx: broadcast::Sender<BookUpdate>,
    rest_semaphore: Arc<Semaphore>,
    symbol: String,
    ws_is_source_of_truth: bool,
) where
    C: GetMarketData + SubscribeMarketData + Send + Sync + 'static,
{
    let mut stream = client.subscribe_l2_update(symbol.clone());
    let mut last_seq: Option<i64> = None;

    while let Some(result) = stream.next().await {
        match result {
            Ok(update) => {
                // Sequence gap detection — only when REST snapshots are the
                // source of truth.  For exchanges whose WS delivers full
                // snapshots (e.g. Hyperliquid) the "sequence" is a timestamp,
                // not a monotonic counter, so gap detection is meaningless.
                if !ws_is_source_of_truth {
                    if let Some(prev) = last_seq {
                        if update.sequence > prev + 1 {
                            last_seq = resnapshot(
                                client.as_ref(),
                                &books,
                                &last_updated,
                                &rest_semaphore,
                                &symbol,
                            )
                            .await;
                            continue;
                        }
                    }
                }
                last_seq = Some(update.sequence);

                // broadcast update before applying
                let _ = update_tx.send(to_book_update(&update));

                let mut book = books.entry(symbol.clone()).or_insert_with(Orderbook::new);
                apply_update(book.value_mut(), &update);
                last_updated.insert(symbol.clone(), Instant::now());
            }
            Err(_) => {
                if ws_is_source_of_truth {
                    // Just resubscribe — next WS message is a full snapshot.
                    stream = client.subscribe_l2_update(symbol.clone());
                } else {
                    // REST re-snapshot before resubscribing.
                    last_seq = resnapshot(
                        client.as_ref(),
                        &books,
                        &last_updated,
                        &rest_semaphore,
                        &symbol,
                    )
                    .await;
                    stream = client.subscribe_l2_update(symbol.clone());
                }
            }
        }
    }
}
