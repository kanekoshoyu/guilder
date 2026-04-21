use guilder_abstraction::{L2Snapshot, L2Update, Side};

use super::storage::PriceLevelStorage;
use super::types::{BookUpdate, Orderbook};

pub(crate) fn apply_update<S: PriceLevelStorage>(book: &mut Orderbook<S>, update: &L2Update) {
    match update.side {
        Side::Ask => book.update_ask(update.price, update.volume),
        Side::Bid => book.update_bid(update.price, update.volume),
    }
}

pub(crate) fn to_book_update(update: &L2Update) -> BookUpdate {
    BookUpdate {
        symbol: update.symbol.clone(),
        side: update.side.clone(),
        price: update.price,
        volume: update.volume,
        exchange_ts_ms: update.sequence as u64,
    }
}

pub(crate) fn apply_snapshot<S: PriceLevelStorage>(book: &mut Orderbook<S>, snapshot: &L2Snapshot) {
    for level in &snapshot.asks {
        book.update_ask(level.price, level.volume);
    }
    for level in &snapshot.bids {
        book.update_bid(level.price, level.volume);
    }
}

pub(crate) fn snapshot_to_book_updates(snapshot: &L2Snapshot) -> Vec<BookUpdate> {
    let asks = snapshot.asks.iter().map(|level| BookUpdate {
        symbol: snapshot.symbol.clone(),
        side: Side::Ask,
        price: level.price,
        volume: level.volume,
        exchange_ts_ms: snapshot.sequence as u64,
    });
    let bids = snapshot.bids.iter().map(|level| BookUpdate {
        symbol: snapshot.symbol.clone(),
        side: Side::Bid,
        price: level.price,
        volume: level.volume,
        exchange_ts_ms: snapshot.sequence as u64,
    });
    asks.chain(bids).collect()
}
