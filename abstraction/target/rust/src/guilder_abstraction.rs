pub trait TestServer {
    fn ping(&self) -> bool;
    fn get_server_time(&self) -> i64;
}

pub trait GetMarketData {
    fn get_symbol(&self) -> Vec<String>;
    fn get_price(&self, symbol: String) -> f64;
}

pub trait ManageOrder {
    fn place_order(&self, x: String, price: i32, volume: i32) -> i32;
    fn change_order(&self, x: String, price: i32, volume: i32) -> i32;
    fn cancel_order(&self, x: String) -> i32;
}

