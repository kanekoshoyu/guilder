use rust_decimal::Decimal;

/// Trait abstracting the backing storage for a single side (bids or asks)
/// of an orderbook. All prices are stored in ascending order so that:
/// - `first()` = lowest price (best for asks)
/// - `last()` = highest price (best for bids)
pub trait PriceLevelStorage {
    fn insert(&mut self, price: Decimal, volume: Decimal);
    fn remove(&mut self, price: &Decimal);

    /// Lowest price level.
    fn first(&self) -> Option<(Decimal, Decimal)>;

    /// Highest price level.
    fn last(&self) -> Option<(Decimal, Decimal)>;

    /// Iterate from lowest price upward.
    fn iter_from_first(&self, depth: Option<usize>) -> Vec<(Decimal, Decimal)>;

    /// Iterate from highest price downward.
    fn iter_from_last(&self, depth: Option<usize>) -> Vec<(Decimal, Decimal)>;

    fn len(&self) -> usize;

    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

// ---------------------------------------------------------------------------
// BTreeMap implementation (current behavior)
// ---------------------------------------------------------------------------

impl PriceLevelStorage for std::collections::BTreeMap<Decimal, Decimal> {
    fn insert(&mut self, price: Decimal, volume: Decimal) {
        if volume == Decimal::ZERO {
            self.remove(&price);
        } else {
            self.insert(price, volume);
        }
    }

    fn remove(&mut self, price: &Decimal) {
        self.remove(price);
    }

    fn first(&self) -> Option<(Decimal, Decimal)> {
        self.first_key_value().map(|(k, v)| (*k, *v))
    }

    fn last(&self) -> Option<(Decimal, Decimal)> {
        self.last_key_value().map(|(k, v)| (*k, *v))
    }

    fn iter_from_first(&self, depth: Option<usize>) -> Vec<(Decimal, Decimal)> {
        match depth {
            Some(n) => self.iter().take(n).map(|(p, v)| (*p, *v)).collect(),
            None => self.iter().map(|(p, v)| (*p, *v)).collect(),
        }
    }

    fn iter_from_last(&self, depth: Option<usize>) -> Vec<(Decimal, Decimal)> {
        match depth {
            Some(n) => self.iter().rev().take(n).map(|(p, v)| (*p, *v)).collect(),
            None => self.iter().rev().map(|(p, v)| (*p, *v)).collect(),
        }
    }

    fn len(&self) -> usize {
        self.len()
    }
}

// ---------------------------------------------------------------------------
// SortedVec implementation
// ---------------------------------------------------------------------------

/// A price-level side backed by a sorted `Vec<(price, volume)>`.
#[derive(Debug, Clone, Default)]
pub struct SortedVec {
    levels: Vec<(Decimal, Decimal)>,
}

impl SortedVec {
    fn partition_point(&self, price: Decimal) -> usize {
        self.levels
            .partition_point(|(p, _)| *p < price)
    }
}

impl PriceLevelStorage for SortedVec {
    fn insert(&mut self, price: Decimal, volume: Decimal) {
        if volume == Decimal::ZERO {
            self.remove(&price);
            return;
        }
        let idx = self.partition_point(price);
        if idx < self.levels.len() && self.levels[idx].0 == price {
            self.levels[idx].1 = volume;
        } else {
            self.levels.insert(idx, (price, volume));
        }
    }

    fn remove(&mut self, price: &Decimal) {
        if let Some(idx) = self.levels.iter().position(|(p, _)| p == price) {
            self.levels.remove(idx);
        }
    }

    fn first(&self) -> Option<(Decimal, Decimal)> {
        self.levels.first().copied()
    }

    fn last(&self) -> Option<(Decimal, Decimal)> {
        self.levels.last().copied()
    }

    fn iter_from_first(&self, depth: Option<usize>) -> Vec<(Decimal, Decimal)> {
        let n = depth.unwrap_or(self.levels.len());
        self.levels.iter().take(n).copied().collect()
    }

    fn iter_from_last(&self, depth: Option<usize>) -> Vec<(Decimal, Decimal)> {
        let n = depth.unwrap_or(self.levels.len());
        self.levels.iter().rev().take(n).copied().collect()
    }

    fn len(&self) -> usize {
        self.levels.len()
    }
}

// ---------------------------------------------------------------------------
// BoundedVec implementation — pre-allocated, keeps only top N levels
// ---------------------------------------------------------------------------

/// A price-level side backed by a pre-allocated `Vec` capped at `DEPTH` levels.
///
/// For asks: keeps the lowest `DEPTH` prices.
/// For bids: keeps the highest `DEPTH` prices.
#[derive(Debug, Clone)]
pub struct BoundedVec<const DEPTH: usize = 20> {
    levels: Vec<(Decimal, Decimal)>,
}

impl<const DEPTH: usize> Default for BoundedVec<DEPTH> {
    fn default() -> Self {
        Self {
            levels: Vec::with_capacity(DEPTH),
        }
    }
}

impl<const DEPTH: usize> BoundedVec<DEPTH> {
    fn partition_point(&self, price: Decimal) -> usize {
        self.levels
            .partition_point(|(p, _)| *p < price)
    }
}

impl<const DEPTH: usize> PriceLevelStorage for BoundedVec<DEPTH> {
    fn insert(&mut self, price: Decimal, volume: Decimal) {
        if volume == Decimal::ZERO {
            self.remove(&price);
            return;
        }
        let idx = self.partition_point(price);
        if idx < self.levels.len() && self.levels[idx].0 == price {
            self.levels[idx].1 = volume;
        } else if self.levels.len() < DEPTH {
            self.levels.insert(idx, (price, volume));
        } else {
            // Full — only insert if it belongs in the top DEPTH
            if price < self.levels.last().unwrap().0 {
                self.levels.pop();
                self.levels.insert(idx, (price, volume));
            }
            // else: price is deeper than our cap, drop it
        }
    }

    fn remove(&mut self, price: &Decimal) {
        if let Some(idx) = self.levels.iter().position(|(p, _)| p == price) {
            self.levels.remove(idx);
        }
    }

    fn first(&self) -> Option<(Decimal, Decimal)> {
        self.levels.first().copied()
    }

    fn last(&self) -> Option<(Decimal, Decimal)> {
        self.levels.last().copied()
    }

    fn iter_from_first(&self, depth: Option<usize>) -> Vec<(Decimal, Decimal)> {
        let n = depth.unwrap_or(self.levels.len());
        self.levels.iter().take(n).copied().collect()
    }

    fn iter_from_last(&self, depth: Option<usize>) -> Vec<(Decimal, Decimal)> {
        let n = depth.unwrap_or(self.levels.len());
        self.levels.iter().rev().take(n).copied().collect()
    }

    fn len(&self) -> usize {
        self.levels.len()
    }
}
