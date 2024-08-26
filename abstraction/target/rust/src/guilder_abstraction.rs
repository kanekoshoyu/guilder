pub trait TestServer {
    fn ping() -> bool;
    fn get_server_time() -> i64;
}

pub trait GetMarketData {
    fn get_symbol() -> Vec<String>;
    fn get_price(symbol: String) -> f64;
}

pub trait ManageOrder {
    fn place_order(x: String, price: i32, volume: i32) -> i32;
    fn change_order(x: String, price: i32, volume: i32) -> i32;
    fn cancel_order(x: String) -> i32;
}
