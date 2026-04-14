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

    // 1. Check spot balance for USDC
    let balances = client
        .get_spot_balance()
        .await
        .expect("get_spot_balance failed");
    let usdc = balances.iter().find(|b| b.coin == "USDC");
    let usdc = usdc.expect("no USDC spot balance found in test wallet");
    assert!(
        usdc.available > Decimal::ZERO,
        "no available USDC in test wallet (total={}, available={}, locked={})",
        usdc.total, usdc.available, usdc.locked
    );
    println!("USDC spot balance: total={} available={} locked={}", usdc.total, usdc.available, usdc.locked);

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
            Some(cloid.clone()),
        )
        .await
        .expect("place_order failed");

    // Cloid is converted to a 16-byte hex string (0x + 32 hex chars) via keccak256
    // to meet Hyperliquid's Cloid format requirement.
    use sha3::{Digest, Keccak256};
    let hash = Keccak256::new_with_prefix(cloid.as_bytes());
    let expected_cloid = format!("0x{}", hex::encode(&hash.finalize()[..16]));

    assert_eq!(order.symbol, "BTC");
    assert_eq!(order.side, OrderSide::Buy);
    assert_eq!(order.cloid.as_deref(), Some(expected_cloid.as_str()));
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
    let cancelled = client.cancel_order(order.order_id).await;
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
