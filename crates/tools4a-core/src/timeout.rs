//! Per-call execution-timeout resolution.
//!
//! Every service orchestrator wraps its protocol call with a deadline.
//! The caller supplies an optional `timeout_secs` (per-call), the operator
//! sets a hard ceiling via `TOOLS4A_MAX_TIMEOUT_SECS` env var or the
//! `[defaults] max_timeout_secs` field of `~/.config/tools4a/config.toml`,
//! and each leaf service supplies a sensible default (e.g. 30s for SQL,
//! 60s for HTTP, 300s for SSH). The resolution:
//!
//! 1. `max_secs` = env > toml > built-in default (`DEFAULT_MAX_TIMEOUT_SECS`)
//! 2. `requested_secs` = caller's value, else the service's default
//! 3. `effective_secs` = `min(requested_secs, max_secs)`; clamping is
//!    silent but recorded so the orchestrator can attach a warning to
//!    the returned `ExecutionResult`.

use crate::{Error, Result};
use std::future::Future;
use std::time::Duration;

/// Built-in ceiling used when neither `TOOLS4A_MAX_TIMEOUT_SECS` nor the
/// TOML `[defaults]` block sets a value. One hour — long enough that the
/// cap won't bite normal use, short enough that a runaway call doesn't
/// pin a tunnel forever.
pub const DEFAULT_MAX_TIMEOUT_SECS: u64 = 3600;

/// Env var name (operator-side override). Highest precedence.
pub const MAX_TIMEOUT_ENV_VAR: &str = "TOOLS4A_MAX_TIMEOUT_SECS";

/// Result of resolving a per-call timeout against the configured max.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EffectiveTimeout {
    /// Final timeout passed to `tokio::time::timeout` (post-clamp).
    pub effective_secs: u64,
    /// What the caller (or service default) asked for, pre-clamp.
    pub requested_secs: u64,
    /// The active ceiling at resolution time.
    pub max_secs: u64,
    /// True iff `requested_secs > max_secs` and we clamped.
    pub clamped: bool,
}

impl EffectiveTimeout {
    pub fn duration(&self) -> Duration {
        Duration::from_secs(self.effective_secs)
    }

    /// Operator-facing warning if the caller's requested timeout was
    /// silently capped. `None` when no clamp happened.
    pub fn clamp_warning(&self) -> Option<String> {
        if !self.clamped {
            return None;
        }
        Some(format!(
            "requested timeout {}s exceeds configured max ({}s); capped to {}s",
            self.requested_secs, self.max_secs, self.effective_secs
        ))
    }
}

/// Resolve the effective per-call timeout.
///
/// - `requested`: caller-supplied value (CLI `--timeout` or MCP
///   `timeout_secs`). `None` means "use service default".
/// - `service_default`: per-service default (e.g. 30 for SQL, 60 for HTTP).
/// - `toml_max`: max from TOML `[defaults].max_timeout_secs`, if loaded.
///   Env var `TOOLS4A_MAX_TIMEOUT_SECS` takes precedence over this.
pub fn resolve_effective_timeout(
    requested: Option<u64>,
    service_default: u64,
    toml_max: Option<u64>,
) -> EffectiveTimeout {
    let max_secs = resolve_max_timeout_secs(toml_max);
    let requested_secs = requested.unwrap_or(service_default).max(1);
    let effective_secs = requested_secs.min(max_secs);
    EffectiveTimeout {
        effective_secs,
        requested_secs,
        max_secs,
        clamped: requested_secs > max_secs,
    }
}

/// Resolve the active max-timeout ceiling. Env > toml > built-in default.
/// Exposed so CLI/MCP layers can show the value in `--help` or surface it
/// in diagnostics.
pub fn resolve_max_timeout_secs(toml_max: Option<u64>) -> u64 {
    if let Ok(raw) = std::env::var(MAX_TIMEOUT_ENV_VAR)
        && let Ok(parsed) = raw.parse::<u64>()
        && parsed > 0
    {
        return parsed;
    }
    toml_max
        .filter(|v| *v > 0)
        .unwrap_or(DEFAULT_MAX_TIMEOUT_SECS)
}

/// Wrap a future with the resolved effective timeout. On expiry, yields
/// `Err(Error::Timeout(effective_secs))`. Threads the inner future's
/// `Result` through unchanged on success.
pub async fn apply_with_timeout<F, T>(deadline: EffectiveTimeout, fut: F) -> Result<T>
where
    F: Future<Output = Result<T>>,
{
    match tokio::time::timeout(deadline.duration(), fut).await {
        Ok(inner) => inner,
        Err(_) => Err(Error::Timeout(deadline)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Env-var test helper: snapshot the var, set it, and restore on drop.
    /// `std::env::set_var` is `unsafe` in Rust 2024 due to thread safety;
    /// these tests only mutate the var in a controlled scope.
    struct EnvGuard {
        key: &'static str,
        prev: Option<String>,
    }

    impl EnvGuard {
        fn set(key: &'static str, value: &str) -> Self {
            let prev = std::env::var(key).ok();
            // SAFETY: tests are single-threaded with respect to this var
            // because `cargo test` runs each in its own task; the guard
            // restores prior state on drop.
            unsafe { std::env::set_var(key, value) };
            Self { key, prev }
        }
        fn clear(key: &'static str) -> Self {
            let prev = std::env::var(key).ok();
            // SAFETY: see `set`.
            unsafe { std::env::remove_var(key) };
            Self { key, prev }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            // SAFETY: see `EnvGuard::set`.
            unsafe {
                if let Some(v) = &self.prev {
                    std::env::set_var(self.key, v);
                } else {
                    std::env::remove_var(self.key);
                }
            }
        }
    }

    #[test]
    fn requested_below_max_passes_through_unchanged() {
        let _g = EnvGuard::clear(MAX_TIMEOUT_ENV_VAR);
        let r = resolve_effective_timeout(Some(15), 30, None);
        assert_eq!(r.effective_secs, 15);
        assert_eq!(r.requested_secs, 15);
        assert!(!r.clamped);
    }

    #[test]
    fn service_default_used_when_no_request() {
        let _g = EnvGuard::clear(MAX_TIMEOUT_ENV_VAR);
        let r = resolve_effective_timeout(None, 30, None);
        assert_eq!(r.effective_secs, 30);
        assert_eq!(r.requested_secs, 30);
        assert!(!r.clamped);
    }

    #[test]
    fn requested_above_max_is_clamped() {
        let _g = EnvGuard::clear(MAX_TIMEOUT_ENV_VAR);
        let r = resolve_effective_timeout(Some(10_000), 30, Some(60));
        assert_eq!(r.effective_secs, 60);
        assert_eq!(r.max_secs, 60);
        assert!(r.clamped);
        assert!(r.clamp_warning().unwrap().contains("capped"));
    }

    #[test]
    fn env_var_overrides_toml_max() {
        let _g = EnvGuard::set(MAX_TIMEOUT_ENV_VAR, "5");
        let r = resolve_effective_timeout(Some(100), 30, Some(60));
        assert_eq!(r.effective_secs, 5);
        assert_eq!(r.max_secs, 5);
        assert!(r.clamped);
    }

    #[test]
    fn zero_or_garbage_env_falls_back_to_toml() {
        let _g = EnvGuard::set(MAX_TIMEOUT_ENV_VAR, "0");
        let r = resolve_effective_timeout(Some(100), 30, Some(60));
        assert_eq!(r.effective_secs, 60);
        assert_eq!(r.max_secs, 60);
    }

    #[test]
    fn requested_zero_is_floored_to_one() {
        let _g = EnvGuard::clear(MAX_TIMEOUT_ENV_VAR);
        // Reject a literal 0 — Duration::from_secs(0) inside tokio::time::timeout
        // returns Pending immediately and is almost certainly user error.
        let r = resolve_effective_timeout(Some(0), 30, None);
        assert_eq!(r.effective_secs, 1);
    }

    #[tokio::test]
    async fn apply_with_timeout_returns_value_on_success() {
        let deadline = EffectiveTimeout {
            effective_secs: 5,
            requested_secs: 5,
            max_secs: 60,
            clamped: false,
        };
        let out: Result<i32> = apply_with_timeout(deadline, async { Ok(42) }).await;
        assert_eq!(out.unwrap(), 42);
    }

    #[tokio::test]
    async fn apply_with_timeout_maps_expiry_to_timeout_error() {
        let deadline = EffectiveTimeout {
            effective_secs: 1,
            requested_secs: 1,
            max_secs: 60,
            clamped: false,
        };
        let result: Result<()> = apply_with_timeout(deadline, async {
            tokio::time::sleep(Duration::from_secs(2)).await;
            Ok(())
        })
        .await;
        match result {
            Err(Error::Timeout(t)) => {
                assert_eq!(t.effective_secs, 1);
                assert!(!t.clamped);
            }
            other => panic!("expected Error::Timeout, got {other:?}"),
        }
    }
}
