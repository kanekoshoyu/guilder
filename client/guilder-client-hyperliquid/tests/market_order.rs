use guilder_abstraction::{
    GetAccountSnapshot, GetMarketData, ManageOrder, OrderSide, OrderType, TimeInForce,
};
use guilder_client_hyperliquid::HyperliquidClient;
use rust_decimal::Decimal;
use std::str::FromStr;

fn require_auth() -> Option<(String, String)> {
    let addr = std::env::var("HYPERLIQUID_WALLET_ADDRESS").ok()?;
    let key = std::env::var("HYPERLIQUID_WALLET_KEY").ok()?;
    Some((addr, key))
}

#[tokio::test]
async fn test_market_order() {
    let Some((addr, key)) = require_auth() else {
        eprintln!("SKIP: set HYPERLIQUID_WALLET_ADDRESS and HYPERLIQUID_WALLET_KEY env vars");
        return;
    };

    let client = HyperliquidClient::with_auth(addr, key);

    let _ = client.cancel_all_order().await;

    let balances = client.get_spot_balance().await.expect("get_spot_balance failed");
    let usdc = balances.iter().find(|b| b.coin == "USDC")
        .expect("no USDC spot balance");
    println!("USDC: total={} available={}", usdc.total, usdc.available);

    let price = client.get_price("BTC".to_string()).await.expect("get_price failed");
    println!("BTC price: {}", price);

    let slippage = Decimal::from_str("0.05").unwrap();
    let order_price = (price * (Decimal::ONE + slippage)).round_dp(0);
    let volume = Decimal::from_str("0.00015").unwrap();

    println!("Placing market buy: price={} qty={}", order_price, volume);

    let cloid = format!("test-mkt-{}", std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis());

    let order = client
        .place_order(
            "BTC".to_string(),
            OrderSide::Buy,
            order_price,
            volume,
            OrderType::Market,
            TimeInForce::Gtc,
            None,
            false,
            Some(cloid),
        )
        .await;

    match order {
        Ok(o) => println!("order placed: oid={}", o.order_id),
        Err(e) => panic!("place_order failed: {}", e),
    }

    let _ = client.cancel_all_order().await;
}
