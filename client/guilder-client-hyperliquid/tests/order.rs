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

/// Full lifecycle test: place a limit order → verify it appears → cancel → verify it's gone.
#[tokio::test]
async fn test_place_and_cancel_limit_order() {
    let Some((addr, key)) = require_auth() else {
        eprintln!("SKIP: set HYPERLIQUID_WALLET_ADDRESS and HYPERLIQUID_WALLET_KEY env vars");
        return;
    };

    let client = HyperliquidClient::with_auth(addr, key);

    // 0. Cancel all existing orders to free up margin
    let _ = client.cancel_all_order().await;

    // 1. Check balance for USDC
    let balances = client
        .get_balance()
        .await
        .expect("get_balance failed");
    let usdc = balances.iter().find(|b| b.token == "USDC");
    let usdc = usdc.expect("no USDC balance found in test wallet");
    assert!(
        usdc.free > Decimal::ZERO,
        "no free USDC in test wallet (equity={}, free={}, hold={})",
        usdc.equity, usdc.free, usdc.hold
    );
    println!("USDC balance: equity={} free={} hold={}", usdc.equity, usdc.free, usdc.hold);

    // 2. Check current BTC price
    let price = client
        .get_price("BTC".to_string())
        .await
        .expect("get_price failed");
    println!("BTC price: {price}");

    // 3. Place a small limit buy at 100 below market
    // 0.00015 BTC ≈ $11 at current prices (minimum order value is $10)
    let buy_price = (price - Decimal::from_str("100").unwrap()).round_dp(0);
    let volume = Decimal::from_str("0.00015").unwrap();
    let cloid = format!(
        "test-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis()
    );

    let order = client
        .place_order(
            "BTC".to_string(),
            OrderSide::Buy,
            buy_price,
            volume,
            OrderType::Limit,
            TimeInForce::Gtc,
            None,
            false,
            Some(cloid.clone()),
        )
        .await
        .expect("place_order failed");

    assert_eq!(order.symbol, "BTC");
    assert_eq!(order.side, OrderSide::Buy);
    assert_eq!(order.cloid.as_deref(), Some(cloid.as_str()));
    println!(
        "order placed: oid={} cloid={} price={} qty={}",
        order.order_id, order.cloid.as_deref().unwrap_or("none"), order.price, order.quantity
    );

    // 4. Verify order appears in open orders
    let open = client
        .get_open_orders()
        .await
        .expect("get_open_orders failed");
    let found = open.iter().find(|o| o.order_id == order.order_id);
    assert!(
        found.is_some(),
        "placed order {} not found in open orders",
        order.order_id
    );
    println!("order confirmed in open orders");

    // 5. Cancel the order
    let cancelled = client.cancel_order_by_cloid(cloid.clone()).await;
    assert!(
        cancelled.is_ok(),
        "cancel_order failed: {:?}",
        cancelled
    );
    println!("order {} cancelled", order.order_id);

    // 6. Verify order is gone
    let open = client
        .get_open_orders()
        .await
        .expect("get_open_orders after cancel failed");
    let still_there = open.iter().find(|o| o.order_id == order.order_id);
    assert!(
        still_there.is_none(),
        "cancelled order {} still appears in open orders",
        order.order_id
    );
    println!("order confirmed removed from open orders");

    // 7. Cleanup: cancel all remaining orders
    let _ = client.cancel_all_order().await;
}
