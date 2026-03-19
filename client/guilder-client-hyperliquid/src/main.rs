use futures_util::StreamExt;
use guilder_abstraction::{GetMarketData, SubscribeMarketData};
use guilder_client_hyperliquid::HyperliquidClient;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::time::timeout;

const SYMBOL: &str = "BTC";
/// Number of unique L2 snapshots (sequence changes) to sample
const L2_SNAPSHOT_COUNT: usize = 10;
/// Number of asset context WS updates to sample
const ASSET_CTX_SAMPLE_COUNT: usize = 5;
/// Max wait per asset context update before declaring stale
const ASSET_CTX_TIMEOUT_SECS: u64 = 30;

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}

#[tokio::main]
async fn main() {
    println!("=== Hyperliquid Data Availability & Freshness Check ===");
    println!("Symbol: {SYMBOL}\n");

    let client = HyperliquidClient::new();

    check_rest_asset_context(&client).await;
    println!();
    check_l2_freshness(&client).await;
    println!();
    check_asset_ctx_freshness(&client).await;
}

/// REST: fetch a single asset context snapshot and report latency + field values.
async fn check_rest_asset_context(client: &HyperliquidClient) {
    println!("--- [1] REST get_asset_context({SYMBOL}) ---");
    let t0 = Instant::now();
    match client.get_asset_context(SYMBOL.to_string()).await {
        Ok(ctx) => {
            let latency = t0.elapsed();
            println!("  mark_price:    {}", ctx.mark_price);
            println!("  open_interest: {}", ctx.open_interest);
            println!("  funding_rate:  {}", ctx.funding_rate);
            println!("  day_volume:    {}", ctx.day_volume);
            println!("  fetch latency: {latency:.0?}");
        }
        Err(e) => println!("  ERROR: {e}"),
    }
}

/// WS: collect L2_SNAPSHOT_COUNT distinct snapshots (grouped by sequence number).
/// Reports per-snapshot age relative to wall clock and gap between snapshots.
async fn check_l2_freshness(client: &HyperliquidClient) {
    println!("--- [2] WS subscribe_l2_update({SYMBOL}) — {L2_SNAPSHOT_COUNT} snapshots ---");

    let mut stream = client.subscribe_l2_update(SYMBOL.to_string());
    let mut prev_wall: Option<Instant> = None;
    let mut prev_seq: Option<i64> = None;
    let mut total_events = 0usize;
    let mut snapshots = 0usize;

    while snapshots < L2_SNAPSHOT_COUNT {
        let event = match timeout(Duration::from_secs(10), stream.next()).await {
            Ok(Some(Ok(e))) => e,
            Ok(Some(Err(e))) => {
                println!("  ERROR: {e}");
                break;
            }
            Ok(None) => {
                println!("  stream closed unexpectedly");
                break;
            }
            Err(_) => {
                println!("  TIMEOUT: no L2 event in 10s");
                break;
            }
        };
        total_events += 1;

        // Only report when the sequence number changes (= new snapshot from the exchange)
        if Some(event.sequence) == prev_seq {
            continue;
        }

        let wall_now = Instant::now();
        let age_ms = now_ms() - event.sequence;
        let wall_gap = prev_wall.map(|p| wall_now.duration_since(p));
        let seq_gap = prev_seq.map(|p| event.sequence - p);

        let wall_gap_str = wall_gap.map_or("    --   ".to_string(), |d| format!("{d:.0?}"));
        let seq_gap_str = seq_gap.map_or("   --  ".to_string(), |d| format!("{d:>6}ms"));

        println!(
            "  #{:02} seq={} age={:>5}ms wall_gap={:>8} seq_gap={}{}",
            snapshots + 1,
            event.sequence,
            age_ms,
            wall_gap_str,
            seq_gap_str,
            if age_ms > 5_000 { "  !! STALE >5s" } else { "" },
        );

        prev_wall = Some(wall_now);
        prev_seq = Some(event.sequence);
        snapshots += 1;
    }

    println!("  => {snapshots} snapshots from {total_events} raw events");
}

/// WS: collect ASSET_CTX_SAMPLE_COUNT asset context updates and report inter-update gaps.
async fn check_asset_ctx_freshness(client: &HyperliquidClient) {
    println!(
        "--- [3] WS subscribe_asset_context({SYMBOL}) — {ASSET_CTX_SAMPLE_COUNT} updates (timeout {ASSET_CTX_TIMEOUT_SECS}s each) ---"
    );

    let mut stream = client.subscribe_asset_context(SYMBOL.to_string());
    let mut prev_wall: Option<Instant> = None;
    let mut count = 0usize;

    while count < ASSET_CTX_SAMPLE_COUNT {
        let ctx = match timeout(Duration::from_secs(ASSET_CTX_TIMEOUT_SECS), stream.next()).await {
            Ok(Some(Ok(c))) => c,
            Ok(Some(Err(e))) => {
                println!("  ERROR: {e}");
                break;
            }
            Ok(None) => {
                println!("  stream closed unexpectedly");
                break;
            }
            Err(_) => {
                println!(
                    "  TIMEOUT: no asset context update in {ASSET_CTX_TIMEOUT_SECS}s — data may be infrequent"
                );
                break;
            }
        };

        let wall_now = Instant::now();
        let wall_gap = prev_wall.map(|p| wall_now.duration_since(p));
        let gap_str = wall_gap.map_or("   --  ".to_string(), |d| format!("{d:.0?}"));

        println!(
            "  #{} mark_px={} oi={} funding={} gap={}",
            count + 1,
            ctx.mark_price,
            ctx.open_interest,
            ctx.funding_rate,
            gap_str,
        );

        prev_wall = Some(wall_now);
        count += 1;
    }

    println!("  => {count} asset context updates received");
}
