use futures_util::StreamExt;
use guilder_abstraction::SubscribeMarketData;
use guilder_client_hyperliquid::HyperliquidClient;
use rust_decimal::Decimal;
use std::time::Duration;
use tokio::time::timeout;

/// Subscribes to BTC L2 updates and asserts at least one event arrives within 5 s.
#[tokio::test]
async fn test_subscribe_l2_update_receives_events() {
    let client = HyperliquidClient::new();
    let mut stream = client.subscribe_l2_update("BTC".to_string());

    let event = timeout(Duration::from_secs(5), stream.next()).await;
    let event = event
        .expect("timed out waiting for L2 update")
        .expect("stream ended early")
        .expect("l2 update should be ok");

    assert_eq!(event.symbol, "BTC");
    assert!(event.price > Decimal::ZERO, "price should be positive");
    assert!(
        event.volume >= Decimal::ZERO,
        "volume should be non-negative"
    );
}

/// Subscribes to BTC trade fills and asserts at least one event arrives within 30 s.
/// Trades may be infrequent so we allow a longer timeout.
#[tokio::test]
async fn test_subscribe_fill_receives_events() {
    let client = HyperliquidClient::new();
    let mut stream = client.subscribe_fill("BTC".to_string());

    let event = timeout(Duration::from_secs(30), stream.next()).await;
    let event = event
        .expect("timed out waiting for fill")
        .expect("stream ended early")
        .expect("fill event should be ok");

    assert_eq!(event.symbol, "BTC");
    assert!(event.price > Decimal::ZERO, "price should be positive");
    assert!(event.volume > Decimal::ZERO, "volume should be positive");
    assert!(event.timestamp_ms > 0, "timestamp should be positive");
}
