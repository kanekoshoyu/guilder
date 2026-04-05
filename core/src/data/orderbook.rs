use super::currency_pair::CurrencyPair;
use guilder_abstraction::Side;
use ordered_float::OrderedFloat;
use std::collections::{BTreeMap, HashMap};

#[derive(Clone, Debug)]
pub struct Orderbook {
    asks: BTreeMap<OrderedFloat<f64>, f64>,
    bids: BTreeMap<OrderedFloat<f64>, f64>,
}

impl Orderbook {
    pub fn new() -> Self {
        Orderbook {
            asks: BTreeMap::new(),
            bids: BTreeMap::new(),
        }
    }

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
        if volume == 0.0 {
            self.asks.remove(&OrderedFloat(price));
            return;
        }
        self.asks.insert(OrderedFloat(price), volume);
    }
    /// update bid price volume
    pub fn update_bid(&mut self, price: f64, volume: f64) {
        if volume == 0.0 {
            self.bids.remove(&OrderedFloat(price));
            return;
        }
        self.bids.insert(OrderedFloat(price), volume);
    }
    /// merge another orderbook
    pub fn merge(&mut self, another: &Orderbook) {
        self.asks.extend(another.asks.clone());
        self.bids.extend(another.bids.clone());
    }

    /// Quote-currency liquidity available within a slippage boundary on one side.
    ///
    /// For asks: sums `price * volume` for all levels up to `best_ask * (1 + slippage_pct)`.
    /// For bids: sums `price * volume` for all levels down to `best_bid * (1 - slippage_pct)`.
    pub fn liquidity(&self, side: Side, slippage_pct: f64) -> Option<f64> {
        match side {
            Side::Ask => {
                let best = self.asks.first_key_value()?;
                let ceiling = best.0.into_inner() * (1.0 + slippage_pct);
                let total = self
                    .asks
                    .iter()
                    .take_while(|(p, _)| p.into_inner() <= ceiling)
                    .map(|(p, v)| p.into_inner() * v)
                    .sum();
                Some(total)
            }
            Side::Bid => {
                let best = self.bids.last_key_value()?;
                let floor = best.0.into_inner() * (1.0 - slippage_pct);
                let total = self
                    .bids
                    .iter()
                    .rev()
                    .take_while(|(p, _)| p.into_inner() >= floor)
                    .map(|(p, v)| p.into_inner() * v)
                    .sum();
                Some(total)
            }
        }
    }

    /// Returns the top `depth` levels per side as `(Side, price, volume)`,
    /// ordered best-to-worst (lowest ask first, highest bid first).
    /// If `depth` is `None`, returns all levels.
    pub fn snapshot(&self, depth: Option<usize>) -> Vec<(Side, f64, f64)> {
        let ask_iter = self.asks.iter().map(|(p, v)| (Side::Ask, p.into_inner(), *v));
        let bid_iter = self
            .bids
            .iter()
            .rev()
            .map(|(p, v)| (Side::Bid, p.into_inner(), *v));
        match depth {
            Some(n) => ask_iter.take(n).chain(bid_iter.take(n)).collect(),
            None => ask_iter.chain(bid_iter).collect(),
        }
    }

    /// Liquidity imbalance ratio: `(B - A) / (B + A)`.
    ///
    /// `top_n` limits to the top N levels per side. `None` uses the full book.
    /// Returns a value in `[-1.0, 1.0]`. Positive = bid-heavy, negative = ask-heavy.
    /// Returns `None` if both sides are empty.
    pub fn imbalance(&self, top_n: Option<usize>) -> Option<f64> {
        let ask_liq: f64 = match top_n {
            Some(n) => self.asks.iter().take(n).map(|(p, v)| p.into_inner() * v).sum(),
            None => self.asks.iter().map(|(p, v)| p.into_inner() * v).sum(),
        };
        let bid_liq: f64 = match top_n {
            Some(n) => self.bids.iter().rev().take(n).map(|(p, v)| p.into_inner() * v).sum(),
            None => self.bids.iter().map(|(p, v)| p.into_inner() * v).sum(),
        };
        let total = bid_liq + ask_liq;
        if total == 0.0 {
            return None;
        }
        Some((bid_liq - ask_liq) / total)
    }
}
impl Default for Orderbook {
    fn default() -> Self {
        Self::new()
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

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_book() -> Orderbook {
        let mut book = Orderbook::new();
        // asks: 100.0 x 1.0, 101.0 x 2.0, 102.0 x 3.0
        book.update_ask(100.0, 1.0);
        book.update_ask(101.0, 2.0);
        book.update_ask(102.0, 3.0);
        // bids: 99.0 x 1.5, 98.0 x 2.5, 97.0 x 3.5
        book.update_bid(99.0, 1.5);
        book.update_bid(98.0, 2.5);
        book.update_bid(97.0, 3.5);
        book
    }

    #[test]
    fn test_liquidity_ask_side() {
        let book = sample_book();
        // best ask = 100.0, 0.5% slippage -> ceiling = 100.5
        // only 100.0 x 1.0 = 100.0 quote
        let liq = book.liquidity(Side::Ask, 0.005).unwrap();
        assert!((liq - 100.0).abs() < 1e-9);
    }

    #[test]
    fn test_liquidity_ask_wider_slippage() {
        let book = sample_book();
        // best ask = 100.0, 2% slippage -> ceiling = 102.0
        // 100.0 x 1.0 + 101.0 x 2.0 + 102.0 x 3.0 = 100 + 202 + 306 = 608
        let liq = book.liquidity(Side::Ask, 0.02).unwrap();
        assert!((liq - 608.0).abs() < 1e-9);
    }

    #[test]
    fn test_liquidity_bid_side() {
        let book = sample_book();
        // best bid = 99.0, 0.5% slippage -> floor = 99.0 * 0.995 = 98.505
        // only 99.0 x 1.5 = 148.5 quote
        let liq = book.liquidity(Side::Bid, 0.005).unwrap();
        assert!((liq - 148.5).abs() < 1e-9);
    }

    #[test]
    fn test_liquidity_empty_book() {
        let book = Orderbook::new();
        assert!(book.liquidity(Side::Ask, 0.01).is_none());
        assert!(book.liquidity(Side::Bid, 0.01).is_none());
    }

    #[test]
    fn test_imbalance_full_book() {
        let book = sample_book();
        // ask liq: 100*1 + 101*2 + 102*3 = 608
        // bid liq: 99*1.5 + 98*2.5 + 97*3.5 = 148.5 + 245 + 339.5 = 733
        // imbalance = (733 - 608) / (733 + 608) = 125 / 1341
        let imb = book.imbalance(None).unwrap();
        let expected = 125.0 / 1341.0;
        assert!((imb - expected).abs() < 1e-9);
    }

    #[test]
    fn test_imbalance_top_1() {
        let book = sample_book();
        // top 1 ask: 100*1 = 100, top 1 bid: 99*1.5 = 148.5
        // imbalance = (148.5 - 100) / (148.5 + 100) = 48.5 / 248.5
        let imb = book.imbalance(Some(1)).unwrap();
        let expected = 48.5 / 248.5;
        assert!((imb - expected).abs() < 1e-9);
    }

    #[test]
    fn test_imbalance_empty_book() {
        let book = Orderbook::new();
        assert!(book.imbalance(None).is_none());
    }

    #[test]
    fn test_update_removes_zero_volume() {
        let mut book = Orderbook::new();
        book.update_ask(100.0, 5.0);
        assert!(book.best_ask().is_some());
        book.update_ask(100.0, 0.0);
        assert!(book.best_ask().is_none());
    }
}
