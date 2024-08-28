use guilder_abstraction::Orderbook as RawOrderBook;
use ordered_float::OrderedFloat;
use std::collections::BTreeMap;

// TODO implement code
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
