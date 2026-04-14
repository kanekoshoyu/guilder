use guilder_abstraction::{L2Update, Side};

use super::types::{BookUpdate, Orderbook};

pub(crate) fn apply_update(book: &mut Orderbook, update: &L2Update) {
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
    }
}
