//! Pre-built agent templates (Phase 3.4).
//!
//! Ships a small library of ready-to-use agents as **definition manifests**
//! (JSON, like skills). A user picks a template and [`instantiate`]s it into
//! their own agent — optionally overriding the id/name — without writing any
//! configuration. Templates carry no genome data (they are just
//! [`AgentDefinition`]s), so they double as the first shareable definitions.

use super::definition::{AgentDefinition, AgentError};

/// The bundled template manifests (compiled into the binary).
const TEMPLATE_JSON: &[&str] = &[
    include_str!("manifests/research-tracker.json"),
    include_str!("manifests/trait-reviewer.json"),
    include_str!("manifests/drug-interaction-watcher.json"),
    include_str!("manifests/new-prs-notifier.json"),
];

/// Parse and validate all bundled agent templates.
///
/// Panics only if a *bundled* manifest is malformed, which a unit test guards
/// against — so this is infallible in practice.
pub fn builtin_templates() -> Vec<AgentDefinition> {
    TEMPLATE_JSON
        .iter()
        .map(|j| AgentDefinition::from_json(j).expect("bundled agent template must be valid"))
        .collect()
}

/// Look up a template by its id.
pub fn find_template(template_id: &str) -> Option<AgentDefinition> {
    builtin_templates().into_iter().find(|t| t.id == template_id)
}

/// Instantiate `template_id` into a concrete agent definition.
///
/// The new agent gets `new_id` (so a user can run several agents from one
/// template) and records the origin template in `template_id`. An optional
/// `name` override renames it. The result is validated before return.
pub fn instantiate(
    template_id: &str,
    new_id: &str,
    name: Option<&str>,
) -> Result<AgentDefinition, AgentError> {
    let mut def = find_template(template_id)
        .ok_or_else(|| AgentError(format!("no such template: {template_id}")))?;
    if new_id.trim().is_empty() {
        return Err(AgentError("new agent id must not be empty".into()));
    }
    def.id = new_id.to_string();
    def.template_id = Some(template_id.to_string());
    if let Some(n) = name {
        if !n.trim().is_empty() {
            def.name = n.to_string();
        }
    }
    def.validate()?;
    Ok(def)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_bundled_templates_are_valid() {
        let ts = builtin_templates();
        assert_eq!(ts.len(), 4);
        for t in &ts {
            assert!(t.validate().is_ok(), "template {} invalid", t.id);
            // every template records itself as its own template origin
            assert_eq!(t.template_id.as_deref(), Some(t.id.as_str()));
        }
    }

    #[test]
    fn templates_have_expected_ids() {
        let ids: Vec<String> = builtin_templates().into_iter().map(|t| t.id).collect();
        assert!(ids.contains(&"org.sovereigndna.agents.research-tracker".to_string()));
        assert!(ids.contains(&"org.sovereigndna.agents.trait-reviewer".to_string()));
        assert!(ids.contains(&"org.sovereigndna.agents.drug-interaction-watcher".to_string()));
        assert!(ids.contains(&"org.sovereigndna.agents.new-prs-notifier".to_string()));
    }

    #[test]
    fn instantiate_sets_id_template_and_name() {
        let d = instantiate(
            "org.sovereigndna.agents.trait-reviewer",
            "org.me.my-trait-agent",
            Some("My Traits"),
        )
        .unwrap();
        assert_eq!(d.id, "org.me.my-trait-agent");
        assert_eq!(
            d.template_id.as_deref(),
            Some("org.sovereigndna.agents.trait-reviewer")
        );
        assert_eq!(d.name, "My Traits");
        // the underlying skill/trigger came from the template
        assert_eq!(d.skill_ids, vec!["org.sovereigndna.traits.core".to_string()]);
    }

    #[test]
    fn instantiate_rejects_unknown_template_or_empty_id() {
        assert!(instantiate("nope", "x", None).is_err());
        assert!(instantiate("org.sovereigndna.agents.trait-reviewer", "  ", None).is_err());
    }
}
