use futures_util::StreamExt;
use guilder_abstraction::SubscribeMarketData;
use guilder_client_hyperliquid::HyperliquidClient;
use rust_decimal::Decimal;
use std::time::Duration;
use tokio::time::timeout;

/// Verifies the stream delivers multiple consecutive events, proving the connection
/// is stable and not just one-shot.
#[tokio::test]
async fn test_l2_stream_delivers_multiple_events() {
    const REQUIRED_EVENTS: usize = 5;
    const TIME_LIMIT: Duration = Duration::from_secs(15);

    let client = HyperliquidClient::new();
    let mut stream = client.subscribe_l2_snapshot("BTC".to_string());
    let mut ok_count = 0;

    let result = timeout(TIME_LIMIT, async {
        while let Some(item) = stream.next().await {
            match item {
                Ok(event) => {
                    assert_eq!(event.symbol, "BTC");
                    assert!(!event.bids.is_empty() || !event.asks.is_empty(), "snapshot should not be empty");
                    assert!(
                        event.bids.first().map(|level| level.price > Decimal::ZERO).unwrap_or(true),
                        "bid price should be positive"
                    );
                    ok_count += 1;
                    if ok_count >= REQUIRED_EVENTS {
                        break;
                    }
                }
                Err(e) => eprintln!("[connection test] stream error (non-fatal): {e}"),
            }
        }
    })
    .await;

    assert!(
        result.is_ok(),
        "timed out — only {ok_count}/{REQUIRED_EVENTS} events received within {TIME_LIMIT:?}"
    );
    assert_eq!(ok_count, REQUIRED_EVENTS);
}

/// Verifies that `Err` items emitted during transient failures do not terminate the stream.
/// The consumer treats errors as non-fatal and keeps receiving `Ok` events afterward.
/// This is the core reconnection contract: errors surface inline, data resumes.
#[tokio::test]
async fn test_stream_continues_after_errors() {
    const REQUIRED_OK: usize = 3;
    const TIME_LIMIT: Duration = Duration::from_secs(20);

    let client = HyperliquidClient::new();
    let mut stream = client.subscribe_l2_snapshot("BTC".to_string());

    let mut ok_count = 0;
    let mut err_count = 0;

    let result = timeout(TIME_LIMIT, async {
        while let Some(item) = stream.next().await {
            match item {
                Ok(_) => {
                    ok_count += 1;
                    if ok_count >= REQUIRED_OK {
                        break;
                    }
                }
                Err(e) => {
                    err_count += 1;
                    eprintln!("[connection test] stream error #{err_count}: {e}");
                    // Do not break — verify stream recovers on its own.
                }
            }
        }
    })
    .await;

    assert!(
        result.is_ok(),
        "stream did not recover — {ok_count} ok events, {err_count} errors within {TIME_LIMIT:?}"
    );
    assert!(
        ok_count >= REQUIRED_OK,
        "expected at least {REQUIRED_OK} ok events, got {ok_count}"
    );
}

/// Verifies that dropping a stream and opening a fresh one connects cleanly.
/// Guards against stale state or leaked connections on the previous session.
#[tokio::test]
async fn test_reconnect_via_new_subscription() {
    const TIME_LIMIT: Duration = Duration::from_secs(10);

    let client = HyperliquidClient::new();

    // First subscription — receive one event then drop.
    {
        let mut stream = client.subscribe_l2_snapshot("BTC".to_string());
        let first = timeout(Duration::from_secs(5), stream.next())
            .await
            .expect("timed out on first subscription")
            .expect("stream ended early on first subscription");
        assert!(first.is_ok(), "expected ok event on first subscription");
    }
    // stream is dropped here

    // Second subscription — should connect fresh with no leftover state.
    let mut stream2 = client.subscribe_l2_snapshot("BTC".to_string());
    let second = timeout(TIME_LIMIT, async {
        loop {
            match stream2.next().await {
                Some(Ok(event)) => break Some(event),
                Some(Err(e)) => eprintln!("[connection test] reconnect error (non-fatal): {e}"),
                None => break None,
            }
        }
    })
    .await
    .expect("timed out waiting for event on second subscription");

    assert!(second.is_some(), "second subscription yielded no events");
    let event = second.unwrap();
    assert_eq!(event.symbol, "BTC");
    assert!(!event.bids.is_empty() || !event.asks.is_empty());
}
