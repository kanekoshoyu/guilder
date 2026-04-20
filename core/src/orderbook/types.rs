use super::storage::PriceLevelStorage;
use guilder_abstraction::Side;
use rust_decimal::Decimal;
use std::collections::BTreeMap;
use std::str::FromStr;

/// Event emitted on each orderbook update.
#[derive(Debug, Clone)]
pub struct BookUpdate {
    pub symbol: String,
    pub side: Side,
    pub price: Decimal,
    pub volume: Decimal,
    /// Exchange-side millisecond timestamp.
    pub exchange_ts_ms: u64,
}

#[derive(Clone, Debug)]
pub struct Orderbook<S: PriceLevelStorage> {
    pub(crate) asks: S,
    pub(crate) bids: S,
}

impl Orderbook<BTreeMap<Decimal, Decimal>> {
    pub fn new() -> Self {
        Orderbook {
            asks: BTreeMap::new(),
            bids: BTreeMap::new(),
        }
    }
}

impl<S: PriceLevelStorage> Orderbook<S> {
    /// lowest priced ask
    pub fn best_ask(&self) -> Option<(Decimal, Decimal)> {
        self.asks.first()
    }
    /// highest priced bid
    pub fn best_bid(&self) -> Option<(Decimal, Decimal)> {
        self.bids.last()
    }
    /// update ask price volume
    pub fn update_ask(&mut self, price: Decimal, volume: Decimal) {
        self.asks.insert(price, volume);
    }
    /// update bid price volume
    pub fn update_bid(&mut self, price: Decimal, volume: Decimal) {
        self.bids.insert(price, volume);
    }
    /// merge another orderbook
    pub fn merge(&mut self, another: &Orderbook<S>)
    where
        S: Clone,
    {
        for (p, v) in another.asks.iter_from_first(None) {
            self.asks.insert(p, v);
        }
        for (p, v) in another.bids.iter_from_last(None) {
            self.bids.insert(p, v);
        }
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
                let (best_price, _) = self.asks.first()?;
                let ceiling = best_price * (one + slippage);
                let mut total = Decimal::ZERO;
                for (p, v) in self.asks.iter_from_first(None) {
                    if p > ceiling {
                        break;
                    }
                    total += p * v;
                }
                Some(total)
            }
            Side::Bid => {
                let (best_price, _) = self.bids.last()?;
                let floor = best_price * (one - slippage);
                let mut total = Decimal::ZERO;
                for (p, v) in self.bids.iter_from_last(None) {
                    if p < floor {
                        break;
                    }
                    total += p * v;
                }
                Some(total)
            }
        }
    }

    /// Returns the top `depth` levels per side as `(Side, price, volume)`,
    /// ordered best-to-worst (lowest ask first, highest bid first).
    /// If `depth` is `None`, returns all levels.
    pub fn snapshot(&self, depth: Option<usize>) -> Vec<(Side, Decimal, Decimal)> {
        let asks = self
            .asks
            .iter_from_first(depth)
            .into_iter()
            .map(|(p, v)| (Side::Ask, p, v));
        let bids = self
            .bids
            .iter_from_last(depth)
            .into_iter()
            .map(|(p, v)| (Side::Bid, p, v));
        match depth {
            Some(n) => asks.take(n).chain(bids.take(n)).collect(),
            None => asks.chain(bids).collect(),
        }
    }

    /// Liquidity imbalance ratio: `(B - A) / (B + A)`.
    ///
    /// `top_n` limits to the top N levels per side. `None` uses the full book.
    /// Returns a value in `[-1.0, 1.0]`. Positive = bid-heavy, negative = ask-heavy.
    /// Returns `None` if both sides are empty.
    pub fn imbalance(&self, top_n: Option<usize>) -> Option<f64> {
        let ask_liq: Decimal = self
            .asks
            .iter_from_first(top_n)
            .iter()
            .map(|(p, v)| *p * *v)
            .sum();
        let bid_liq: Decimal = self
            .bids
            .iter_from_last(top_n)
            .iter()
            .map(|(p, v)| *p * *v)
            .sum();
        let total = bid_liq + ask_liq;
        if total == Decimal::ZERO {
            return None;
        }
        let ratio = (bid_liq - ask_liq) / total;
        f64::from_str(&ratio.to_string()).ok()
    }
}

impl<S: PriceLevelStorage> Default for Orderbook<S>
where
    S: Default,
{
    fn default() -> Self {
        Self {
            asks: S::default(),
            bids: S::default(),
        }
    }
}
