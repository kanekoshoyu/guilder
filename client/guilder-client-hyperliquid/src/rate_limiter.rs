//! Rate limiters for the Hyperliquid REST API.
//!
//! ## IP-based (`RestRateLimiter`)
//! Tracks an aggregated weight budget of **1 200 per minute** in a 60-second sliding window.
//!
//! ## Address-based (`AddressRateLimiter`)
//! Tracks the per-address action budget. Each address starts with 10 000 requests and accrues
//! 1 request per 1 USDC traded. When exhausted, 1 request is allowed every 10 seconds.
//! Cancels use a higher limit: `min(limit + 100_000, limit * 2)`.
//! A batch of n orders/cancels counts as n requests against this limit.
//!
//! ## Interface
//! Both limiters share the same interface:
//! - `acquire(...)` → `Result<(), RateLimitError>` — returns `Err` immediately if throttled.
//! - `acquire_blocking(...)` — retries on `Err`, logging each wait via `tracing::warn`.

use std::collections::VecDeque;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

// ── IP-based constants ────────────────────────────────────────────────────────

const WINDOW: Duration = Duration::from_secs(60);
pub const MAX_WEIGHT: u32 = 1200;

// ── Address-based constants ───────────────────────────────────────────────────

pub const ADDR_INITIAL_BUFFER: u64 = 10_000;
const ADDR_THROTTLE_INTERVAL: Duration = Duration::from_secs(10);

// ── Error ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct RateLimitError {
    pub retry_after: Duration,
}

impl std::fmt::Display for RateLimitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "rate limited, retry after {:?}", self.retry_after)
    }
}

impl std::error::Error for RateLimitError {}

// ── IP-based limiter ──────────────────────────────────────────────────────────

pub struct RestRateLimiter {
    entries: Mutex<VecDeque<(Instant, u32)>>,
}

impl Default for RestRateLimiter {
    fn default() -> Self {
        Self::new()
    }
}

impl RestRateLimiter {
    pub fn new() -> Self {
        RestRateLimiter {
            entries: Mutex::new(VecDeque::new()),
        }
    }

    /// Returns `Err(RateLimitError)` immediately if the weight budget is exhausted.
    pub async fn acquire(&self, weight: u32) -> Result<(), RateLimitError> {
        let mut entries = self.entries.lock().await;
        let now = Instant::now();

        while let Some(&(t, _)) = entries.front() {
            if now.duration_since(t) >= WINDOW {
                entries.pop_front();
            } else {
                break;
            }
        }

        let used: u32 = entries.iter().map(|(_, w)| w).sum();
        if used + weight <= MAX_WEIGHT {
            entries.push_back((now, weight));
            return Ok(());
        }

        let oldest = entries.front().unwrap().0;
        let retry_after = WINDOW.saturating_sub(now.duration_since(oldest));
        Err(RateLimitError { retry_after })
    }

    /// Retries on `Err`, logging each wait. Resolves once the request is accepted.
    pub async fn acquire_blocking(&self, weight: u32) {
        loop {
            match self.acquire(weight).await {
                Ok(()) => return,
                Err(e) => {
                    tracing::warn!(
                        retry_after_ms = e.retry_after.as_millis(),
                        "REST rate limited"
                    );
                    tokio::time::sleep(e.retry_after).await;
                }
            }
        }
    }
}

// ── Address-based limiter ─────────────────────────────────────────────────────

/// Tracks the per-address action budget.
///
/// Call [`record_fill`] when a fill is confirmed to grow the budget.
/// Call [`acquire`] before every action (not info) request.
/// Pass `is_cancel = true` for cancel actions to apply the higher cancel limit.
/// Pass the actual batch size as `count` for batched requests.
pub struct AddressRateLimiter {
    inner: Mutex<AddrState>,
}

struct AddrState {
    budget: u64,
    consumed: u64,
    last_throttled: Option<Instant>,
}

impl Default for AddressRateLimiter {
    fn default() -> Self {
        Self::new()
    }
}

impl AddressRateLimiter {
    pub fn new() -> Self {
        AddressRateLimiter {
            inner: Mutex::new(AddrState {
                budget: ADDR_INITIAL_BUFFER,
                consumed: 0,
                last_throttled: None,
            }),
        }
    }

    /// Increase the budget by `usdc_volume` requests (1 request per 1 USDC traded).
    pub async fn record_fill(&self, usdc_volume: u64) {
        let mut s = self.inner.lock().await;
        s.budget = s.budget.saturating_add(usdc_volume);
    }

    fn cancel_limit(default: u64) -> u64 {
        (default + 100_000).min(default * 2)
    }

    /// Returns `Err(RateLimitError)` immediately if the address budget is exhausted.
    pub async fn acquire(&self, count: u64, is_cancel: bool) -> Result<(), RateLimitError> {
        let mut s = self.inner.lock().await;
        let effective_limit = if is_cancel {
            Self::cancel_limit(s.budget)
        } else {
            s.budget
        };

        if s.consumed + count <= effective_limit {
            s.consumed += count;
            return Ok(());
        }

        // Budget exhausted — compute throttle wait.
        let now = Instant::now();
        let retry_after = match s.last_throttled {
            Some(t) => ADDR_THROTTLE_INTERVAL.saturating_sub(now.duration_since(t)),
            None => Duration::ZERO,
        };

        if retry_after.is_zero() {
            if count > 1 {
                return Err(RateLimitError {
                    retry_after: ADDR_THROTTLE_INTERVAL,
                });
            }
            s.last_throttled = Some(now);
            s.consumed += 1;
            return Ok(());
        }

        Err(RateLimitError { retry_after })
    }

    /// Retries on `Err`, logging each wait. Resolves once the request is accepted.
    pub async fn acquire_blocking(&self, count: u64, is_cancel: bool) {
        loop {
            match self.acquire(count, is_cancel).await {
                Ok(()) => return,
                Err(e) => {
                    tracing::warn!(
                        retry_after_ms = e.retry_after.as_millis(),
                        "address rate limited"
                    );
                    tokio::time::sleep(e.retry_after).await;
                }
            }
        }
    }
}
