trait GetMarketData {
    fn get_price(x: String, y: i32) -> i32;
}

trait OrderPlace {
    fn get_price(x: String, price: i32, volume: i32) -> i32;
}

trait OrderChange {
    fn get_price(x: String, price: i32, volume: i32) -> i32;
}

trait OrderCancel {
    fn get_price(x: String) -> i32;
}

