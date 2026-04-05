use crate::Orderbook;
use guilder_abstraction::{L2Update, Side};

pub(crate) fn apply_update(book: &mut Orderbook, update: &L2Update) {
    let price = f64_from_decimal(update.price);
    let volume = f64_from_decimal(update.volume);
    match update.side {
        Side::Ask => book.update_ask(price, volume),
        Side::Bid => book.update_bid(price, volume),
    }
}

pub(crate) fn f64_from_decimal(d: rust_decimal::Decimal) -> f64 {
    use std::str::FromStr;
    f64::from_str(&d.to_string()).unwrap_or(0.0)
}
