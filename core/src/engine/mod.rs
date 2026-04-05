mod convert;
mod sync;

use crate::Orderbook;
use dashmap::DashMap;
use guilder_abstraction::{GetMarketData, Side, SubscribeMarketData};
use std::sync::Arc;

use convert::apply_update;
use sync::sync_loop;

pub struct OrderbookEngine<C> {
    client: Arc<C>,
    books: Arc<DashMap<String, Orderbook>>,
}

impl<C> OrderbookEngine<C>
where
    C: GetMarketData + SubscribeMarketData + Send + Sync + 'static,
{
    pub fn new(client: C) -> Self {
        OrderbookEngine {
            client: Arc::new(client),
            books: Arc::new(DashMap::new()),
        }
    }

    /// Subscribe to all symbols returned by `get_symbol()` and block, syncing
    /// orderbooks until all streams end. Caller should `tokio::task::spawn_local`
    /// or run this on a `LocalSet` since trait async fns may not be `Send`.
    pub async fn track_all(&self) -> Result<(), String> {
        let symbols = self.client.get_symbol().await?;
        self.track(symbols).await
    }

    /// Subscribe to a specific set of symbols and block, syncing orderbooks.
    pub async fn track(&self, symbols: Vec<String>) -> Result<(), String> {
        // snapshot all first
        for symbol in &symbols {
            let snapshot = self.client.get_l2_orderbook(symbol.clone()).await?;
            let mut book = Orderbook::new();
            for update in &snapshot {
                apply_update(&mut book, update);
            }
            self.books.insert(symbol.clone(), book);
        }

        // run all sync loops concurrently via select
        let mut futures: Vec<_> = symbols
            .into_iter()
            .map(|symbol| Box::pin(sync_loop(Arc::clone(&self.client), Arc::clone(&self.books), symbol)))
            .collect();

        // poll all futures until they all complete
        loop {
            if futures.is_empty() {
                break;
            }
            let (_, _, remaining) = futures::future::select_all(futures).await;
            futures = remaining;
        }

        Ok(())
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
