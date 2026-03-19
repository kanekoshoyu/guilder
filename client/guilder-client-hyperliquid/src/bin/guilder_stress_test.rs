/// Guilder stress test for Hyperliquid.
///
/// Subscribes to all available l2Book symbols over a single WS connection and hammers
/// the REST endpoint concurrently to exercise the rate-limiter.
///
/// Run with:
///   cargo run --bin guilder-stress-test
///
/// REST limits respected
///   - 1 200 weight / minute enforced by the shared RestRateLimiter
use futures_util::{SinkExt, StreamExt};
use guilder_abstraction::GetMarketData;
use guilder_client_hyperliquid::{rate_limiter::RestRateLimiter, HyperliquidClient};
use serde::Deserialize;
use std::{
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use tokio::sync::mpsc;
use tokio::time::interval;
use tokio_tungstenite::{connect_async, tungstenite::Message};

// ── Configuration ────────────────────────────────────────────────────────────

/// Concurrent REST tasks sharing the 1 200 weight/min budget.
const REST_WORKERS: usize = 8;

/// How often to print the combined status line.
const STATUS_INTERVAL: Duration = Duration::from_secs(30);

const HYPERLIQUID_WS_URL: &str = "wss://api.hyperliquid.xyz/ws";
const HYPERLIQUID_INFO_URL: &str = "https://api.hyperliquid.xyz/info";

// ── Counters ─────────────────────────────────────────────────────────────────

#[derive(Default)]
struct WsCounters {
    events_ok: AtomicU64,
    errors: AtomicU64,
    reconnects: AtomicU64,
}

#[derive(Default)]
struct RestCounters {
    ok: AtomicU64,
    err: AtomicU64,
    throttle_waits: AtomicU64,
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn ts() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let h = (secs / 3600) % 24;
    let m = (secs / 60) % 60;
    let s = secs % 60;
    format!("{h:02}:{m:02}:{s:02}Z")
}

fn fmt_dur(d: Duration) -> String {
    let s = d.as_secs();
    if s < 60 {
        format!("{s}s")
    } else if s < 3600 {
        format!("{}m{}s", s / 60, s % 60)
    } else {
        format!("{}h{}m", s / 3600, (s % 3600) / 60)
    }
}

// ── Minimal WS deserialization ────────────────────────────────────────────────

#[derive(Deserialize)]
struct WsEnvelope {
    channel: String,
    #[serde(default)]
    data: serde_json::Value,
}

#[derive(Deserialize)]
struct WsBook {
    coin: String,
}

// ── Multiplexed WS worker ─────────────────────────────────────────────────────

/// Opens one WS connection and subscribes to l2Book for every symbol in `symbols`.
/// Parsed events are forwarded as `Ok(symbol)` through `tx`; errors as `Err(msg)`.
/// Reconnects on any failure with exponential back-off.
async fn ws_multi_worker(
    conn_id: usize,
    symbols: Vec<String>,
    tx: mpsc::Sender<Result<String, String>>,
    counters: Arc<WsCounters>,
) {
    let mut backoff_secs: u64 = 1;
    loop {
        let ws = match connect_async(HYPERLIQUID_WS_URL).await {
            Ok((ws, _)) => ws,
            Err(e) => {
                let msg = format!("conn-{conn_id} connect failed: {e}");
                counters.errors.fetch_add(1, Ordering::Relaxed);
                println!("[{}] {msg}", ts());
                let _ = tx.send(Err(msg)).await;
                tokio::time::sleep(Duration::from_secs(backoff_secs)).await;
                backoff_secs = (backoff_secs * 2).min(60);
                continue;
            }
        };

        let (mut sink, mut stream) = ws.split();

        // Subscribe to all symbols on this connection.
        let mut sub_failed = false;
        for symbol in &symbols {
            let msg = serde_json::json!({
                "method": "subscribe",
                "subscription": {"type": "l2Book", "coin": symbol}
            })
            .to_string();
            if let Err(e) = sink.send(Message::Text(msg.into())).await {
                let err = format!("conn-{conn_id} subscribe({symbol}) failed: {e}");
                counters.errors.fetch_add(1, Ordering::Relaxed);
                println!("[{}] {err}", ts());
                let _ = tx.send(Err(err)).await;
                sub_failed = true;
                break;
            }
        }
        if sub_failed {
            tokio::time::sleep(Duration::from_secs(backoff_secs)).await;
            backoff_secs = (backoff_secs * 2).min(60);
            continue;
        }

        // Successfully connected and subscribed — reset backoff.
        println!(
            "[{}] conn-{conn_id} up: {} subscriptions",
            ts(),
            symbols.len()
        );
        backoff_secs = 1;

        let mut ping_interval = tokio::time::interval_at(
            tokio::time::Instant::now() + Duration::from_secs(50),
            Duration::from_secs(50),
        );
        let mut pong_deadline: Option<std::pin::Pin<Box<tokio::time::Sleep>>> = None;
        let mut in_error_run = false;

        loop {
            tokio::select! {
                _ = ping_interval.tick() => {
                    if let Err(e) = sink.send(Message::Text(r#"{"method":"ping"}"#.to_string().into())).await {
                        let err = format!("conn-{conn_id} ping failed: {e}");
                        counters.errors.fetch_add(1, Ordering::Relaxed);
                        println!("[{}] {err} — reconnecting", ts());
                        let _ = tx.send(Err(err)).await;
                        break;
                    }
                    pong_deadline = Some(Box::pin(tokio::time::sleep(Duration::from_secs(30))));
                }
                _ = async { pong_deadline.as_mut().unwrap().await }, if pong_deadline.is_some() => {
                    let err = format!("conn-{conn_id} pong timeout — reconnecting");
                    counters.errors.fetch_add(1, Ordering::Relaxed);
                    println!("[{}] {err}", ts());
                    let _ = tx.send(Err(err)).await;
                    break;
                }
                msg = stream.next() => {
                    match msg {
                        None | Some(Err(_)) => {
                            let err = format!("conn-{conn_id} stream closed — reconnecting");
                            counters.errors.fetch_add(1, Ordering::Relaxed);
                            println!("[{}] {err}", ts());
                            let _ = tx.send(Err(err)).await;
                            break;
                        }
                        Some(Ok(Message::Ping(data))) => {
                            let _ = sink.send(Message::Pong(data)).await;
                        }
                        Some(Ok(Message::Close(_))) => {
                            let err = format!("conn-{conn_id} server closed — reconnecting");
                            counters.errors.fetch_add(1, Ordering::Relaxed);
                            println!("[{}] {err}", ts());
                            let _ = tx.send(Err(err)).await;
                            break;
                        }
                        Some(Ok(Message::Text(text))) => {
                            let Ok(env) = serde_json::from_str::<WsEnvelope>(&text) else { continue };
                            match env.channel.as_str() {
                                "pong" => { pong_deadline = None; }
                                "subscriptionResponse" => {}
                                "l2Book" => {
                                    if let Ok(book) = serde_json::from_value::<WsBook>(env.data) {
                                        if in_error_run {
                                            counters.reconnects.fetch_add(1, Ordering::Relaxed);
                                            println!("[{}] conn-{conn_id} reconnected", ts());
                                            in_error_run = false;
                                        }
                                        counters.events_ok.fetch_add(1, Ordering::Relaxed);
                                        let _ = tx.try_send(Ok(book.coin));
                                    }
                                }
                                _ => {}
                            }
                        }
                        Some(Ok(_)) => {}
                    }
                }
            }
        }

        let _ = in_error_run; // set on reconnect path inside loop
        tokio::time::sleep(Duration::from_secs(backoff_secs)).await;
        backoff_secs = (backoff_secs * 2).min(60);
    }
}

// ── REST worker ───────────────────────────────────────────────────────────────

async fn rest_worker(id: usize, counters: Arc<RestCounters>, limiter: Arc<RestRateLimiter>) {
    let http = reqwest::Client::new();
    loop {
        let t0 = Instant::now();
        limiter.acquire_blocking(2).await; // allMids → weight 2
        if t0.elapsed() > Duration::from_millis(50) {
            counters.throttle_waits.fetch_add(1, Ordering::Relaxed);
        }

        let result = http
            .post(HYPERLIQUID_INFO_URL)
            .json(&serde_json::json!({"type": "allMids"}))
            .send()
            .await;

        match result {
            Ok(r) if r.status().is_success() => {
                counters.ok.fetch_add(1, Ordering::Relaxed);
            }
            Ok(r) => {
                counters.err.fetch_add(1, Ordering::Relaxed);
                println!("[{}] REST worker-{id} HTTP {}", ts(), r.status());
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
            Err(e) => {
                counters.err.fetch_add(1, Ordering::Relaxed);
                println!("[{}] REST worker-{id} error: {e}", ts());
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        }
    }
}

// ── Fetch all symbols ─────────────────────────────────────────────────────────

async fn fetch_all_symbols(client: &HyperliquidClient) -> Vec<String> {
    loop {
        match client.get_symbol().await {
            Ok(symbols) => {
                println!("[{}] fetched {} symbols", ts(), symbols.len());
                return symbols;
            }
            Err(e) => {
                println!("[{}] failed to fetch symbols: {e} — retrying in 5s", ts());
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        }
    }
}

// ── Main ──────────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() {
    println!("[{}] guilder-stress-test starting", ts());

    let client = HyperliquidClient::new();

    let symbols = fetch_all_symbols(&client).await;

    println!("[{}] {} symbols → 1 connection", ts(), symbols.len());
    println!(
        "[{}] REST workers: {REST_WORKERS}  (shared 1 200 weight/min budget)",
        ts()
    );
    println!("[{}] status every {STATUS_INTERVAL:?}", ts());
    println!();

    // ── WS worker ─────────────────────────────────────────────────────────────
    let ws_counters = Arc::new(WsCounters::default());
    let (tx, mut rx) = mpsc::channel::<Result<String, String>>(4096);

    {
        let tx2 = tx.clone();
        let ctr = Arc::clone(&ws_counters);
        tokio::spawn(async move {
            ws_multi_worker(0, symbols, tx2, ctr).await;
        });
    }
    drop(tx); // main no longer holds a sender

    // Drain the event channel in a background task (we only care about counters).
    tokio::spawn(async move { while rx.recv().await.is_some() {} });

    // ── REST workers ──────────────────────────────────────────────────────────
    let rest_counters = Arc::new(RestCounters::default());
    let shared_limiter = Arc::new(RestRateLimiter::new());

    for id in 0..REST_WORKERS {
        let ctr = Arc::clone(&rest_counters);
        let lim = Arc::clone(&shared_limiter);
        tokio::spawn(async move { rest_worker(id, ctr, lim).await });
    }

    // ── Status loop ───────────────────────────────────────────────────────────
    let start = Instant::now();
    let mut tick = interval(STATUS_INTERVAL);
    tick.tick().await;

    let mut prev_ws_ok: u64 = 0;
    let mut prev_rest_ok: u64 = 0;

    loop {
        tick.tick().await;
        let uptime = fmt_dur(start.elapsed());

        let ws_ok = ws_counters.events_ok.load(Ordering::Relaxed);
        let ws_err = ws_counters.errors.load(Ordering::Relaxed);
        let ws_rec = ws_counters.reconnects.load(Ordering::Relaxed);

        let rest_ok = rest_counters.ok.load(Ordering::Relaxed);
        let rest_err = rest_counters.err.load(Ordering::Relaxed);
        let throttles = rest_counters.throttle_waits.load(Ordering::Relaxed);

        let ws_rate = (ws_ok - prev_ws_ok) as f64 / STATUS_INTERVAL.as_secs_f64();
        let rest_rate = (rest_ok - prev_rest_ok) as f64 / STATUS_INTERVAL.as_secs_f64();
        prev_ws_ok = ws_ok;
        prev_rest_ok = rest_ok;

        println!(
            "[{}] STATUS  uptime={uptime}  \
             ws_ok={ws_ok} (+{ws_rate:.1}/s)  ws_err={ws_err}  ws_reconnects={ws_rec}  \
             rest_ok={rest_ok} (+{rest_rate:.1}/s)  rest_err={rest_err}  throttle_waits={throttles}",
            ts()
        );
    }
}
