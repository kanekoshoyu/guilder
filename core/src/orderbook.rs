use super::currency_pair::CurrencyPair;
use guilder_abstraction::Orderbook as RawOrderBook;
use ordered_float::OrderedFloat;
use std::collections::{BTreeMap, HashMap};

#[derive(Clone, Debug)]
pub struct Orderbook {
    asks: BTreeMap<OrderedFloat<f64>, f64>,
    bids: BTreeMap<OrderedFloat<f64>, f64>,
}

impl From<RawOrderBook> for Orderbook {
    fn from(raw: RawOrderBook) -> Self {
        let asks = raw
            .asks
            .clone()
            .into_iter()
            .map(|(key, value)| (key.into(), value))
            .collect();
        let bids = raw
            .bids
            .clone()
            .into_iter()
            .map(|(key, value)| (key.into(), value))
            .collect();
        Orderbook { asks, bids }
    }
}
impl Orderbook {
    /// lowest priced ask
    pub fn best_ask(&self) -> Option<(&OrderedFloat<f64>, &f64)> {
        self.asks.first_key_value()
    }
    /// highest priced bid
    pub fn best_bid(&self) -> Option<(&OrderedFloat<f64>, &f64)> {
        self.bids.last_key_value()
    }
    /// update ask price volume
    pub fn update_ask(&mut self, price: f64, volume: f64) {
        if volume.eq(&0.0) {
            self.asks.remove(&OrderedFloat(price));
        }
        self.asks.insert(OrderedFloat(price), volume);
    }
    /// update bid price volume
    pub fn update_bid(&mut self, price: f64, volume: f64) {
        if volume.eq(&0.0) {
            self.bids.remove(&OrderedFloat(price));
        }
        self.bids.insert(OrderedFloat(price), volume);
    }
    /// merge another orderbook
    pub fn merge(&mut self, another: &Orderbook) {
        self.asks.extend(another.asks.clone());
        self.bids.extend(another.bids.clone());
    }
}
pub struct IndexOrderbook {
    inner: HashMap<CurrencyPair, Orderbook>,
}
impl IndexOrderbook {
    /// lowest priced ask
    pub fn best_ask(&self, pair: &CurrencyPair) -> Option<(&OrderedFloat<f64>, &f64)> {
        let book = self.inner.get(pair)?;
        book.best_ask()
    }
    /// lowest priced ask
    pub fn best_bid(&self, pair: &CurrencyPair) -> Option<(&OrderedFloat<f64>, &f64)> {
        let book = self.inner.get(pair)?;
        book.best_bid()
    }
}
