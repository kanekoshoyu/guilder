use super::currency_pair::CurrencyPair;
use guilder_abstraction::Side;
use rust_decimal::Decimal;
use std::collections::HashMap;
use std::collections::BTreeMap;
use std::str::FromStr;

/// Event emitted on each orderbook update.
#[derive(Debug, Clone)]
pub struct BookUpdate {
    pub symbol: String,
    pub side: Side,
    pub price: Decimal,
    pub volume: Decimal,
}

#[derive(Clone, Debug)]
pub struct Orderbook {
    pub(crate) asks: BTreeMap<Decimal, Decimal>,
    pub(crate) bids: BTreeMap<Decimal, Decimal>,
}

impl Orderbook {
    pub fn new() -> Self {
        Orderbook {
            asks: BTreeMap::new(),
            bids: BTreeMap::new(),
        }
    }

    /// lowest priced ask
    pub fn best_ask(&self) -> Option<(&Decimal, &Decimal)> {
        self.asks.first_key_value()
    }
    /// highest priced bid
    pub fn best_bid(&self) -> Option<(&Decimal, &Decimal)> {
        self.bids.last_key_value()
    }
    /// update ask price volume
    pub fn update_ask(&mut self, price: Decimal, volume: Decimal) {
        if volume == Decimal::ZERO {
            self.asks.remove(&price);
            return;
        }
        self.asks.insert(price, volume);
    }
    /// update bid price volume
    pub fn update_bid(&mut self, price: Decimal, volume: Decimal) {
        if volume == Decimal::ZERO {
            self.bids.remove(&price);
            return;
        }
        self.bids.insert(price, volume);
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
    pub fn liquidity(&self, side: Side, slippage_pct: f64) -> Option<Decimal> {
        let slippage: Decimal = slippage_pct.to_string().parse().ok()?;
        let one = Decimal::ONE;
        match side {
            Side::Ask => {
                let (best_price, _) = self.asks.first_key_value()?;
                let ceiling = *best_price * (one + slippage);
                let total: Decimal = self
                    .asks
                    .iter()
                    .take_while(|(p, _)| **p <= ceiling)
                    .map(|(p, v)| *p * *v)
                    .sum();
                Some(total)
            }
            Side::Bid => {
                let (best_price, _) = self.bids.last_key_value()?;
                let floor = *best_price * (one - slippage);
                let total: Decimal = self
                    .bids
                    .iter()
                    .rev()
                    .take_while(|(p, _)| **p >= floor)
                    .map(|(p, v)| *p * *v)
                    .sum();
                Some(total)
            }
        }
    }

    /// Returns the top `depth` levels per side as `(Side, price, volume)`,
    /// ordered best-to-worst (lowest ask first, highest bid first).
    /// If `depth` is `None`, returns all levels.
    pub fn snapshot(&self, depth: Option<usize>) -> Vec<(Side, Decimal, Decimal)> {
        let ask_iter = self.asks.iter().map(|(p, v)| (Side::Ask, *p, *v));
        let bid_iter = self.bids.iter().rev().map(|(p, v)| (Side::Bid, *p, *v));
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
        let ask_liq: Decimal = match top_n {
            Some(n) => self
                .asks
                .iter()
                .take(n)
                .map(|(p, v)| *p * *v)
                .sum(),
            None => self.asks.iter().map(|(p, v)| *p * *v).sum(),
        };
        let bid_liq: Decimal = match top_n {
            Some(n) => self
                .bids
                .iter()
                .rev()
                .take(n)
                .map(|(p, v)| *p * *v)
                .sum(),
            None => self.bids.iter().map(|(p, v)| *p * *v).sum(),
        };
        let total = bid_liq + ask_liq;
        if total == Decimal::ZERO {
            return None;
        }
        let ratio = (bid_liq - ask_liq) / total;
        f64::from_str(&ratio.to_string()).ok()
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
    pub fn best_ask(&self, pair: &CurrencyPair) -> Option<(&Decimal, &Decimal)> {
        let book = self.inner.get(pair)?;
        book.best_ask()
    }
    /// highest priced bid
    pub fn best_bid(&self, pair: &CurrencyPair) -> Option<(&Decimal, &Decimal)> {
        let book = self.inner.get(pair)?;
        book.best_bid()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    fn sample_book() -> Orderbook {
        let mut book = Orderbook::new();
        // asks: 100 x 1, 101 x 2, 102 x 3
        book.update_ask(dec!(100), dec!(1));
        book.update_ask(dec!(101), dec!(2));
        book.update_ask(dec!(102), dec!(3));
        // bids: 99 x 1.5, 98 x 2.5, 97 x 3.5
        book.update_bid(dec!(99), dec!(1.5));
        book.update_bid(dec!(98), dec!(2.5));
        book.update_bid(dec!(97), dec!(3.5));
        book
    }

    #[test]
    fn test_liquidity_ask_side() {
        let book = sample_book();
        // best ask = 100, 0.5% slippage -> ceiling = 100.5
        // only 100 x 1 = 100 quote
        let liq = book.liquidity(Side::Ask, 0.005).unwrap();
        assert_eq!(liq, dec!(100));
    }

    #[test]
    fn test_liquidity_ask_wider_slippage() {
        let book = sample_book();
        // best ask = 100, 2% slippage -> ceiling = 102
        // 100*1 + 101*2 + 102*3 = 100 + 202 + 306 = 608
        let liq = book.liquidity(Side::Ask, 0.02).unwrap();
        assert_eq!(liq, dec!(608));
    }

    #[test]
    fn test_liquidity_bid_side() {
        let book = sample_book();
        // best bid = 99, 0.5% slippage -> floor = 99 * 0.995 = 98.505
        // only 99 x 1.5 = 148.5 quote
        let liq = book.liquidity(Side::Bid, 0.005).unwrap();
        assert_eq!(liq, dec!(148.5));
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
        book.update_ask(dec!(100), dec!(5));
        assert!(book.best_ask().is_some());
        book.update_ask(dec!(100), Decimal::ZERO);
        assert!(book.best_ask().is_none());
    }
}
