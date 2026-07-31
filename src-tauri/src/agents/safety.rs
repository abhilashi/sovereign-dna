//! Safety rails for agents (Phase 3.6).
//!
//! Long-running, proactive agents multiply the app's egress surface, so this
//! module gives the run/scheduler layers three enforceable guards:
//!
//! * [`TokenBucket`] — rate-limit outbound NCBI/PubMed calls (etiquette; avoids
//!   IP bans) and any other bursty egress;
//! * [`SpendCap`] — cap LLM spend for Claude-backed agents so a misbehaving
//!   background agent can't run up a bill;
//! * [`EgressGuard`] — enforce, **in the Rust layer** (the webview CSP does not
//!   cover backend `reqwest` calls — spec §6.3), an **endpoint allowlist** plus
//!   the **rsID-only egress invariant**: only public identifiers may ever leave
//!   the device. This reuses the ledger's genome-safety definition so the two
//!   layers can never disagree.
//!
//! All pure and deterministic (time is injected), so unit-testable.

use super::ledger::is_public_identifier;

/// A classic token-bucket rate limiter. Time is supplied by the caller (epoch
/// milliseconds) so it is deterministic under test.
#[derive(Debug, Clone)]
pub struct TokenBucket {
    capacity: f64,
    refill_per_sec: f64,
    tokens: f64,
    last_ms: i64,
}

impl TokenBucket {
    /// A bucket holding up to `capacity` tokens, refilling `refill_per_sec`.
    pub fn new(capacity: f64, refill_per_sec: f64, now_ms: i64) -> Self {
        Self {
            capacity: capacity.max(0.0),
            refill_per_sec: refill_per_sec.max(0.0),
            tokens: capacity.max(0.0),
            last_ms: now_ms,
        }
    }

    /// NCBI E-utilities etiquette default: 3 requests/second, small burst.
    pub fn ncbi_default(now_ms: i64) -> Self {
        Self::new(3.0, 3.0, now_ms)
    }

    fn refill(&mut self, now_ms: i64) {
        if now_ms > self.last_ms {
            let elapsed_sec = (now_ms - self.last_ms) as f64 / 1000.0;
            self.tokens = (self.tokens + elapsed_sec * self.refill_per_sec).min(self.capacity);
            self.last_ms = now_ms;
        }
    }

    /// Try to take one token. Returns `true` if the call may proceed.
    pub fn try_acquire(&mut self, now_ms: i64) -> bool {
        self.refill(now_ms);
        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }

    /// Tokens currently available (after refilling to `now_ms`).
    pub fn available(&mut self, now_ms: i64) -> f64 {
        self.refill(now_ms);
        self.tokens
    }
}

/// A running spend cap for remote-LLM-backed agents.
#[derive(Debug, Clone)]
pub struct SpendCap {
    pub limit_usd: f64,
    pub spent_usd: f64,
}

impl SpendCap {
    pub fn new(limit_usd: f64) -> Self {
        Self {
            limit_usd: limit_usd.max(0.0),
            spent_usd: 0.0,
        }
    }

    /// Charge `cost_usd`; denied (and nothing charged) if it would exceed the cap.
    pub fn try_charge(&mut self, cost_usd: f64) -> Result<(), String> {
        let cost = cost_usd.max(0.0);
        if self.spent_usd + cost > self.limit_usd + f64::EPSILON {
            return Err(format!(
                "LLM spend cap reached: {:.4} + {:.4} would exceed limit {:.4} USD",
                self.spent_usd, cost, self.limit_usd
            ));
        }
        self.spent_usd += cost;
        Ok(())
    }

    pub fn remaining(&self) -> f64 {
        (self.limit_usd - self.spent_usd).max(0.0)
    }
}

/// Enforces the Rust-layer egress allowlist + the rsID-only invariant.
#[derive(Debug, Clone)]
pub struct EgressGuard {
    allowed_endpoints: Vec<String>,
}

impl Default for EgressGuard {
    /// The CSP-aligned default: the reference/research endpoints the app already
    /// uses, plus the local Ollama host and (for opt-in Claude agents) Anthropic.
    fn default() -> Self {
        Self {
            allowed_endpoints: vec![
                "eutils.ncbi.nlm.nih.gov".into(),
                "ftp.ncbi.nlm.nih.gov".into(),
                "www.ebi.ac.uk".into(),
                "api.anthropic.com".into(),
                "localhost".into(),
                "127.0.0.1".into(),
            ],
        }
    }
}

impl EgressGuard {
    pub fn with_endpoints(allowed: Vec<String>) -> Self {
        Self {
            allowed_endpoints: allowed,
        }
    }

    /// Whether `endpoint` (host, optionally `host:port`) is on the allowlist.
    /// Matches by host, ignoring any `:port` suffix.
    pub fn is_endpoint_allowed(&self, endpoint: &str) -> bool {
        let host = endpoint.split('/').next().unwrap_or(endpoint);
        let host = host.rsplit_once(':').map(|(h, _)| h).unwrap_or(host);
        self.allowed_endpoints
            .iter()
            .any(|a| a.eq_ignore_ascii_case(host))
    }

    /// Full outbound check: endpoint must be allowlisted **and** every identifier
    /// must be a public rsID/coordinate (never a genotype).
    pub fn check(&self, endpoint: &str, identifiers: &[String]) -> Result<(), String> {
        if !self.is_endpoint_allowed(endpoint) {
            return Err(format!("egress endpoint not on allowlist: {endpoint}"));
        }
        enforce_rsid_only(identifiers)
    }
}

/// The rsID-only egress invariant: reject any identifier that is not a public
/// dbSNP rsID / coordinate. Shared with [`crate::agents::ledger`] so the safety
/// and audit layers use one definition of "safe to send".
pub fn enforce_rsid_only(identifiers: &[String]) -> Result<(), String> {
    for id in identifiers {
        if !is_public_identifier(id) {
            return Err(format!(
                "refusing egress: '{id}' is not a public rsID/coordinate"
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_bucket_limits_burst_then_refills() {
        let mut b = TokenBucket::new(3.0, 3.0, 0);
        // burst of 3 succeeds, 4th fails at t=0
        assert!(b.try_acquire(0));
        assert!(b.try_acquire(0));
        assert!(b.try_acquire(0));
        assert!(!b.try_acquire(0));
        // after 1s, ~3 tokens refill
        assert!(b.try_acquire(1000));
        assert!(b.available(1000) >= 1.0);
    }

    #[test]
    fn token_bucket_caps_at_capacity() {
        let mut b = TokenBucket::new(2.0, 5.0, 0);
        // idle for 10s but capacity caps tokens at 2
        assert!((b.available(10_000) - 2.0).abs() < 1e-9);
    }

    #[test]
    fn spend_cap_denies_over_limit_and_charges_within() {
        let mut c = SpendCap::new(1.0);
        assert!(c.try_charge(0.6).is_ok());
        assert!(c.try_charge(0.6).is_err()); // would exceed
        assert!(c.try_charge(0.4).is_ok()); // exactly to the limit
        assert!(c.remaining() < 1e-9);
    }

    #[test]
    fn egress_guard_allows_known_endpoints_only() {
        let g = EgressGuard::default();
        assert!(g.is_endpoint_allowed("eutils.ncbi.nlm.nih.gov"));
        assert!(g.is_endpoint_allowed("localhost:11434"));
        assert!(g.is_endpoint_allowed("api.anthropic.com"));
        assert!(!g.is_endpoint_allowed("evil.example.com"));
    }

    #[test]
    fn egress_guard_enforces_rsid_only() {
        let g = EgressGuard::default();
        assert!(g.check("eutils.ncbi.nlm.nih.gov", &["rs429358".into()]).is_ok());
        // genotype smuggled into identifiers → rejected
        assert!(g
            .check("eutils.ncbi.nlm.nih.gov", &["rs1".into(), "AG".into()])
            .is_err());
        // disallowed endpoint → rejected even with a valid rsID
        assert!(g.check("evil.example.com", &["rs1".into()]).is_err());
    }

    #[test]
    fn enforce_rsid_only_rejects_genotypes() {
        assert!(enforce_rsid_only(&["rs1".into(), "chr1:123".into()]).is_ok());
        assert!(enforce_rsid_only(&["AA".into()]).is_err());
    }
}
