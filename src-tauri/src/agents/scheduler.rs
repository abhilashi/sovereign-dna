//! Trigger / scheduler engine (Phase 3.2).
//!
//! Decides *when* an agent should run. Two trigger families:
//! * **time-based** — [`Trigger::Interval`], a cron-like cadence;
//! * **event-based** — [`Trigger::OnReferenceUpdate`] / [`Trigger::OnNewMatchedArticle`],
//!   which fire when a fleet [`FleetEvent`] newer than the agent's last run
//!   occurs (the "new since last scan" idea from `research::digest`, generalised).
//!
//! Pure logic: it takes each agent's definition, its last-run timestamp and the
//! recent event list, and returns which agents are due. The execution substrate
//! (Phase 3.8) — an in-app background tick vs. a sidecar daemon — calls
//! [`due_agents`] on a timer and runs the results; that decision is documented in
//! the module docs but does not change this logic.

use chrono::{DateTime, Duration, Utc};

use super::definition::{AgentDefinition, Trigger};

/// A fleet event an agent may be waiting on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FleetEvent {
    /// A reference database finished updating (`source` = `clinvar`, ...).
    ReferenceUpdated { source: String, at: String },
    /// A new PubMed article matched some user variants.
    NewMatchedArticle { at: String },
}

impl FleetEvent {
    fn at(&self) -> &str {
        match self {
            FleetEvent::ReferenceUpdated { at, .. } => at,
            FleetEvent::NewMatchedArticle { at } => at,
        }
    }
}

fn parse(ts: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(ts.trim())
        .ok()
        .map(|d| d.with_timezone(&Utc))
}

/// Whether an event occurred strictly after `since` (or `since` is absent/unpar-
/// seable → any event counts).
fn event_after(event_at: &str, since: Option<&str>) -> bool {
    match (parse(event_at), since.and_then(parse)) {
        (Some(e), Some(s)) => e > s,
        (Some(_), None) => true, // never ran → any event is new
        _ => false,              // unparseable event → conservatively not new
    }
}

/// Whether an agent with `trigger` and `last_run_at` is due at `now`, given the
/// recent `events`.
pub fn is_due(
    trigger: &Trigger,
    last_run_at: Option<&str>,
    now: &str,
    events: &[FleetEvent],
) -> bool {
    match trigger {
        // Manual agents never run automatically.
        Trigger::Manual => false,
        Trigger::Interval { every_hours } => {
            let Some(now_dt) = parse(now) else {
                return false;
            };
            match last_run_at.and_then(parse) {
                None => true, // never run → due now
                Some(last) => {
                    let due_at = last + Duration::hours(*every_hours as i64);
                    now_dt >= due_at
                }
            }
        }
        Trigger::OnReferenceUpdate { source } => events.iter().any(|e| {
            matches!(e, FleetEvent::ReferenceUpdated { source: s, .. } if s.eq_ignore_ascii_case(source))
                && event_after(e.at(), last_run_at)
        }),
        Trigger::OnNewMatchedArticle => events.iter().any(|e| {
            matches!(e, FleetEvent::NewMatchedArticle { .. }) && event_after(e.at(), last_run_at)
        }),
    }
}

/// For an interval trigger, the next scheduled run time (RFC3339), or `None` for
/// manual/event triggers.
pub fn next_run_at(trigger: &Trigger, last_run_at: Option<&str>, now: &str) -> Option<String> {
    match trigger {
        Trigger::Interval { every_hours } => {
            let base = last_run_at.and_then(parse).or_else(|| parse(now))?;
            Some((base + Duration::hours(*every_hours as i64)).to_rfc3339())
        }
        _ => None,
    }
}

/// Given each agent's definition + last-run timestamp, return the ids of agents
/// due to run at `now`. Disabled/invalid definitions are skipped by the caller.
pub fn due_agents(
    agents: &[(AgentDefinition, Option<String>)],
    now: &str,
    events: &[FleetEvent],
) -> Vec<String> {
    agents
        .iter()
        .filter(|(def, last)| is_due(&def.trigger, last.as_deref(), now, events))
        .map(|(def, _)| def.id.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::definition::{DataScope, LlmConfig};

    fn def(id: &str, trigger: Trigger) -> AgentDefinition {
        AgentDefinition {
            schema_version: 1,
            id: id.into(),
            version: "1.0.0".into(),
            name: "A".into(),
            description: "d".into(),
            skill_ids: vec!["s".into()],
            data_scope: DataScope::default(),
            llm: LlmConfig::None,
            trigger,
            template_id: None,
            instructions: String::new(),
            disclaimer: "not medical advice".into(),
        }
    }

    #[test]
    fn manual_is_never_due() {
        assert!(!is_due(&Trigger::Manual, None, "2026-07-16T00:00:00Z", &[]));
    }

    #[test]
    fn interval_due_when_never_run() {
        assert!(is_due(
            &Trigger::Interval { every_hours: 24 },
            None,
            "2026-07-16T00:00:00Z",
            &[]
        ));
    }

    #[test]
    fn interval_respects_cadence() {
        let t = Trigger::Interval { every_hours: 24 };
        // 12h after last run → not due
        assert!(!is_due(&t, Some("2026-07-16T00:00:00Z"), "2026-07-16T12:00:00Z", &[]));
        // 24h after → due
        assert!(is_due(&t, Some("2026-07-16T00:00:00Z"), "2026-07-17T00:00:00Z", &[]));
        // 25h after → due
        assert!(is_due(&t, Some("2026-07-16T00:00:00Z"), "2026-07-17T01:00:00Z", &[]));
    }

    #[test]
    fn next_run_at_is_last_plus_interval() {
        let n = next_run_at(
            &Trigger::Interval { every_hours: 6 },
            Some("2026-07-16T00:00:00Z"),
            "2026-07-16T03:00:00Z",
        );
        assert!(n.unwrap().starts_with("2026-07-16T06:00:00"));
        assert!(next_run_at(&Trigger::Manual, None, "2026-07-16T00:00:00Z").is_none());
    }

    #[test]
    fn reference_update_trigger_fires_on_matching_new_event() {
        let t = Trigger::OnReferenceUpdate {
            source: "clinvar".into(),
        };
        let events = vec![FleetEvent::ReferenceUpdated {
            source: "clinvar".into(),
            at: "2026-07-16T10:00:00Z".into(),
        }];
        // event after last run → due
        assert!(is_due(&t, Some("2026-07-16T00:00:00Z"), "2026-07-16T11:00:00Z", &events));
        // event before last run → not due
        assert!(!is_due(&t, Some("2026-07-16T12:00:00Z"), "2026-07-16T13:00:00Z", &events));
        // wrong source → not due
        let t2 = Trigger::OnReferenceUpdate {
            source: "gwas_catalog".into(),
        };
        assert!(!is_due(&t2, Some("2026-07-16T00:00:00Z"), "2026-07-16T11:00:00Z", &events));
    }

    #[test]
    fn new_article_trigger_fires() {
        let t = Trigger::OnNewMatchedArticle;
        let events = vec![FleetEvent::NewMatchedArticle {
            at: "2026-07-16T10:00:00Z".into(),
        }];
        assert!(is_due(&t, None, "2026-07-16T11:00:00Z", &events));
        assert!(!is_due(&t, Some("2026-07-16T12:00:00Z"), "2026-07-16T13:00:00Z", &events));
    }

    #[test]
    fn due_agents_selects_the_right_set() {
        let agents = vec![
            (def("manual", Trigger::Manual), None),
            (def("daily", Trigger::Interval { every_hours: 24 }), None),
            (
                def("recent", Trigger::Interval { every_hours: 24 }),
                Some("2026-07-16T00:00:00Z".to_string()),
            ),
        ];
        let due = due_agents(&agents, "2026-07-16T06:00:00Z", &[]);
        assert_eq!(due, vec!["daily".to_string()]); // manual excluded, recent not yet due
    }
}
