mod convert;
mod error;
mod sync;

use crate::Orderbook;
use dashmap::DashMap;
use guilder_abstraction::{GetMarketData, Side, SubscribeMarketData};
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc};
use tokio::time::Instant;

use convert::apply_update;
use sync::sync_loop;

pub use error::EngineError;

/// Event emitted on each orderbook update.
#[derive(Debug, Clone)]
pub struct BookUpdate {
    pub symbol: String,
    pub side: Side,
    pub price: f64,
    pub volume: f64,
}

pub struct OrderbookEngine<C> {
    client: Arc<C>,
    books: Arc<DashMap<String, Orderbook>>,
    last_updated: Arc<DashMap<String, Instant>>,
    add_tx: mpsc::UnboundedSender<Vec<String>>,
    add_rx: Option<mpsc::UnboundedReceiver<Vec<String>>>,
    update_tx: broadcast::Sender<BookUpdate>,
}

impl<C> OrderbookEngine<C>
where
    C: GetMarketData + SubscribeMarketData + Send + Sync + 'static,
{
    pub fn new(client: C) -> Self {
        let (add_tx, add_rx) = mpsc::unbounded_channel();
        let (update_tx, _) = broadcast::channel(4096);
        OrderbookEngine {
            client: Arc::new(client),
            books: Arc::new(DashMap::new()),
            last_updated: Arc::new(DashMap::new()),
            add_tx,
            add_rx: Some(add_rx),
            update_tx,
        }
    }

    /// Subscribe to a broadcast channel of orderbook updates.
    /// Returns a receiver that yields `BookUpdate` on each price-level change.
    pub fn subscribe_updates(&self) -> broadcast::Receiver<BookUpdate> {
        self.update_tx.subscribe()
    }

    /// Subscribe to all symbols returned by `get_symbol()` and block, syncing
    /// orderbooks until all streams end. Caller should `tokio::task::spawn_local`
    /// or run this on a `LocalSet` since trait async fns may not be `Send`.
    pub async fn track_all(&mut self) -> Result<(), EngineError> {
        let symbols = self.client.get_symbol().await?;
        self.track(symbols).await
    }

    /// Subscribe to a specific set of symbols and block, syncing orderbooks.
    pub async fn track(&mut self, symbols: Vec<String>) -> Result<(), EngineError> {
        self.snapshot_symbols(&symbols).await?;

        let mut futures: Vec<_> = symbols
            .into_iter()
            .map(|symbol| {
                Box::pin(sync_loop(
                    Arc::clone(&self.client),
                    Arc::clone(&self.books),
                    Arc::clone(&self.last_updated),
                    self.update_tx.clone(),
                    symbol,
                ))
            })
            .collect();

        // take the receiver — only one track() call drives the add-loop
        let mut add_rx = self.add_rx.take();

        loop {
            if futures.is_empty() && add_rx.is_none() {
                break;
            }

            if futures.is_empty() {
                // only hot-add channel remains
                if let Some(rx) = &mut add_rx {
                    match rx.recv().await {
                        Some(new_symbols) => {
                            if let Err(e) = self.snapshot_symbols(&new_symbols).await {
                                eprintln!("track_additional snapshot error: {e}");
                                continue;
                            }
                            for symbol in new_symbols {
                                futures.push(Box::pin(sync_loop(
                                    Arc::clone(&self.client),
                                    Arc::clone(&self.books),
                                    Arc::clone(&self.last_updated),
                                    self.update_tx.clone(),
                                    symbol,
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
                                if let Err(e) = self.snapshot_symbols(&new_symbols).await {
                                    eprintln!("track_additional snapshot error: {e}");
                                    continue;
                                }
                                for symbol in new_symbols {
                                    futures.push(Box::pin(sync_loop(
                                        Arc::clone(&self.client),
                                        Arc::clone(&self.books),
                                        Arc::clone(&self.last_updated),
                                        self.update_tx.clone(),
                                        symbol,
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
        self.add_tx.send(symbols).map_err(|e| {
            EngineError::Client(format!("track loop not running: {e}"))
        })
    }

    /// Snapshot and insert initial orderbooks for a batch of symbols (parallel).
    async fn snapshot_symbols(&self, symbols: &[String]) -> Result<(), EngineError> {
        let futs: Vec<_> = symbols
            .iter()
            .map(|sym| {
                let client = Arc::clone(&self.client);
                let sym = sym.clone();
                async move {
                    let snapshot = client.get_l2_orderbook(sym.clone()).await?;
                    Ok::<_, String>((sym, snapshot))
                }
            })
            .collect();

        let results = futures::future::join_all(futs).await;
        for result in results {
            let (sym, snapshot) = result?;
            let mut book = Orderbook::new();
            for update in &snapshot {
                apply_update(&mut book, update);
            }
            self.books.insert(sym.clone(), book);
            self.last_updated.insert(sym, Instant::now());
        }

        Ok(())
    }

    /// Returns the top `depth` levels per side for a symbol.
    /// If `depth` is `None`, returns all levels.
    pub fn snapshot(
        &self,
        symbol: &str,
        depth: Option<usize>,
    ) -> Option<Vec<(Side, f64, f64)>> {
        let book = self.books.get(symbol)?;
        Some(book.snapshot(depth))
    }

    /// When was this symbol's orderbook last updated?
    pub fn last_updated(&self, symbol: &str) -> Option<Instant> {
        self.last_updated.get(symbol).map(|r| *r.value())
    }

    /// Health status for all tracked symbols: `(symbol, last_updated)`.
    pub fn health(&self) -> Vec<(String, Instant)> {
        self.last_updated
            .iter()
            .map(|entry| (entry.key().clone(), *entry.value()))
            .collect()
    }

    /// Quote-currency liquidity within a slippage boundary on one side.
    pub fn liquidity(&self, symbol: &str, side: Side, slippage_pct: f64) -> Option<f64> {
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
