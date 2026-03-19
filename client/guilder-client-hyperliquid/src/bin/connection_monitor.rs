/// Long-running connection monitor.
///
/// Subscribes to the BTC L2 update stream and runs indefinitely, printing a
/// status line every 30 s and logging every error/reconnect as it happens.
///
/// Run with:
///   cargo run --bin connection_monitor
///
/// Metrics tracked:
///   events_ok    — total successful L2 events received
///   errors       — total Err items yielded (each one usually means a reconnect attempt)
///   reconnects   — number of times the stream recovered from an error back to Ok
///   uptime       — wall time since the monitor started
///   since_last   — time since the last successful event
use futures_util::StreamExt;
use guilder_abstraction::SubscribeMarketData;
use guilder_client_hyperliquid::HyperliquidClient;
use std::time::{Duration, Instant};
use tokio::time::interval;

const SYMBOL: &str = "BTC";
const STATUS_INTERVAL: Duration = Duration::from_secs(30);

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

fn fmt_duration(d: Duration) -> String {
    let secs = d.as_secs();
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3600 {
        format!("{}m{}s", secs / 60, secs % 60)
    } else {
        format!("{}h{}m", secs / 3600, (secs % 3600) / 60)
    }
}

#[tokio::main]
async fn main() {
    println!("[{}] connection_monitor starting — symbol={SYMBOL}", ts());
    println!("[{}] status printed every {STATUS_INTERVAL:?}, errors logged immediately", ts());
    println!();

    let client = HyperliquidClient::new();
    let mut stream = client.subscribe_l2_update(SYMBOL.to_string());

    let start = Instant::now();
    let mut status_tick = interval(STATUS_INTERVAL);
    status_tick.tick().await; // consume the immediate first tick

    let mut events_ok: u64 = 0;
    let mut errors: u64 = 0;
    let mut reconnects: u64 = 0;
    let mut last_ok: Option<Instant> = None;
    let mut in_error_run = false; // true while we're seeing consecutive errors

    loop {
        tokio::select! {
            item = stream.next() => {
                match item {
                    None => {
                        println!("[{}] stream terminated unexpectedly — exiting", ts());
                        break;
                    }
                    Some(Ok(event)) => {
                        if in_error_run {
                            // First ok after a run of errors = reconnect succeeded
                            reconnects += 1;
                            println!(
                                "[{}] reconnected  ok_total={events_ok} errors={errors} reconnects={reconnects}",
                                ts()
                            );
                            in_error_run = false;
                        }
                        events_ok += 1;
                        last_ok = Some(Instant::now());
                        // Sanity-check the data
                        if event.symbol != SYMBOL {
                            println!("[{}] WARN unexpected symbol: {}", ts(), event.symbol);
                        }
                    }
                    Some(Err(e)) => {
                        errors += 1;
                        in_error_run = true;
                        let since = last_ok.map(|t| format!(" ({}s since last ok)", t.elapsed().as_secs())).unwrap_or_default();
                        println!("[{}] ERROR{since}: {e}", ts());
                    }
                }
            }
            _ = status_tick.tick() => {
                let uptime = fmt_duration(start.elapsed());
                let since_last = last_ok
                    .map(|t| fmt_duration(t.elapsed()))
                    .unwrap_or_else(|| "never".to_string());
                println!(
                    "[{}] STATUS  uptime={uptime}  events_ok={events_ok}  errors={errors}  reconnects={reconnects}  since_last={since_last}",
                    ts()
                );
            }
        }
    }
}
