//! Per-turn heuristic effort router (v0).
//!
//! When `[effort_router]` is enabled and effort is not pinned (CLI `/effort` /
//! `--effort` / persona), score the user prompt and stamp
//! `sampling_config.reasoning_effort` to `low` | `medium` | `high` — no model
//! swap. Heuristics only; no LLM classifier.
//!
//! Precedence (callers enforce pin):
//! explicit pin > router > `default_reasoning_effort` > catalog high.

use serde::{Deserialize, Serialize};
use xai_grok_sampling_types::ReasoningEffort;

/// Router-selectable efforts. Catalog may offer more (xhigh/max); v0 only
/// routes among these three.
pub const ROUTER_EFFORTS: [ReasoningEffort; 3] = [
    ReasoningEffort::Low,
    ReasoningEffort::Medium,
    ReasoningEffort::High,
];

/// Model-info / session meta key: when `true`, the TUI shows
/// `low (auto)` / `medium (auto)` and the per-turn router is active.
pub const EFFORT_AUTO_META_KEY: &str = "effortAuto";

/// `[effort_router]` in config.toml.
///
/// ```toml
/// [effort_router]
/// enabled = true
/// preference = 3   # 1..=5; 3 is neutral, higher biases toward high effort
/// floor = "low"
/// ceiling = "high"
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct EffortRouterConfig {
    /// When false, the router never mutates sampling config.
    pub enabled: bool,
    /// Soft bias on a 1..=5 scale (clamped). 3 = neutral.
    pub preference: u8,
    /// Minimum effort the router may choose (among low|medium|high).
    #[serde(default = "default_floor")]
    pub floor: ReasoningEffort,
    /// Maximum effort the router may choose (among low|medium|high).
    #[serde(default = "default_ceiling")]
    pub ceiling: ReasoningEffort,
}

fn default_floor() -> ReasoningEffort {
    ReasoningEffort::Low
}

fn default_ceiling() -> ReasoningEffort {
    ReasoningEffort::High
}

impl Default for EffortRouterConfig {
    fn default() -> Self {
        Self {
            // Fork default: auto-route instead of always shipping catalog high.
            enabled: true,
            preference: 3,
            floor: ReasoningEffort::Low,
            ceiling: ReasoningEffort::High,
        }
    }
}

impl EffortRouterConfig {
    /// Normalize floor/ceiling into the router triad and ensure floor <= ceiling.
    pub fn clamped_bounds(&self) -> (ReasoningEffort, ReasoningEffort) {
        let floor = clamp_to_router_triad(self.floor);
        let ceiling = clamp_to_router_triad(self.ceiling);
        if effort_rank(floor) <= effort_rank(ceiling) {
            (floor, ceiling)
        } else {
            (ceiling, floor)
        }
    }
}

/// Map a free-form effort into the router triad (low|medium|high).
pub fn clamp_to_router_triad(effort: ReasoningEffort) -> ReasoningEffort {
    match effort {
        ReasoningEffort::None | ReasoningEffort::Minimal | ReasoningEffort::Low => {
            ReasoningEffort::Low
        }
        ReasoningEffort::Medium => ReasoningEffort::Medium,
        ReasoningEffort::High | ReasoningEffort::Xhigh | ReasoningEffort::Max => {
            ReasoningEffort::High
        }
    }
}

fn effort_rank(effort: ReasoningEffort) -> u8 {
    match clamp_to_router_triad(effort) {
        ReasoningEffort::Low => 0,
        ReasoningEffort::Medium => 1,
        ReasoningEffort::High => 2,
        _ => 1,
    }
}

fn effort_from_rank(rank: i8) -> ReasoningEffort {
    match rank.clamp(0, 2) {
        0 => ReasoningEffort::Low,
        1 => ReasoningEffort::Medium,
        _ => ReasoningEffort::High,
    }
}

fn clamp_rank_to_bounds(rank: i8, floor: ReasoningEffort, ceiling: ReasoningEffort) -> ReasoningEffort {
    let lo = effort_rank(floor) as i8;
    let hi = effort_rank(ceiling) as i8;
    effort_from_rank(rank.clamp(lo, hi))
}

/// Pure prompt → effort map. Ignores `enabled` and pin; callers gate those.
///
/// Scoring (v0 heuristics):
/// - base rank medium (1)
/// - length: very short → −1; long → +1
/// - simple-chat keywords → −1
/// - hard/coding/debug keywords → +1 (each bucket once)
/// - preference: `preference - 3` added to rank (−2..=+2)
/// - clamp to floor..=ceiling within low|medium|high
pub fn route_effort(prompt: &str, cfg: &EffortRouterConfig) -> ReasoningEffort {
    let (floor, ceiling) = cfg.clamped_bounds();
    let mut rank: i8 = 1; // medium

    let trimmed = prompt.trim();
    let char_len = trimmed.chars().count();
    if char_len < 40 {
        rank -= 1;
    } else if char_len > 400 {
        rank += 1;
    }

    let lower = trimmed.to_ascii_lowercase();

    if has_simple_signal(&lower, char_len) {
        rank -= 1;
    }
    if has_hard_signal(&lower) {
        rank += 1;
    }
    if has_coding_signal(&lower) {
        rank += 1;
    }

    let pref = (cfg.preference.clamp(1, 5) as i8) - 3;
    rank += pref;

    clamp_rank_to_bounds(rank, floor, ceiling)
}

/// When enabled and not pinned, return the routed effort; otherwise `None`.
pub fn maybe_route_effort(
    prompt: &str,
    cfg: &EffortRouterConfig,
    pinned: bool,
) -> Option<ReasoningEffort> {
    if !cfg.enabled || pinned {
        return None;
    }
    Some(route_effort(prompt, cfg))
}

/// Status / log label: `medium` or `medium (auto)` when the router chose.
pub fn format_effort_status(effort: ReasoningEffort, auto: bool) -> String {
    if auto {
        format!("{} (auto)", effort.as_str())
    } else {
        effort.as_str().to_string()
    }
}

/// True when set_session_model meta asks to re-enable the per-turn router.
pub fn is_effort_auto_meta(meta: Option<&serde_json::Map<String, serde_json::Value>>) -> bool {
    meta.and_then(|m| m.get(xai_grok_sampling_types::REASONING_EFFORT_META_KEY))
        .and_then(|v| v.as_str())
        .is_some_and(|s| s.eq_ignore_ascii_case("auto"))
}

fn has_simple_signal(lower: &str, char_len: usize) -> bool {
    if char_len > 120 {
        return false;
    }
    const SIMPLE: &[&str] = &[
        "hi",
        "hello",
        "hey",
        "thanks",
        "thank you",
        "ty",
        "ok",
        "okay",
        "what time",
        "who are you",
        "good morning",
        "good night",
        "lol",
        "yes",
        "no",
        "yep",
        "nope",
    ];
    SIMPLE.iter().any(|k| {
        lower == *k
            || lower.starts_with(&format!("{k} "))
            || lower.starts_with(&format!("{k}!"))
            || lower.starts_with(&format!("{k}?"))
            || lower.ends_with(&format!(" {k}"))
    })
}

fn has_hard_signal(lower: &str) -> bool {
    const HARD: &[&str] = &[
        "architect",
        "architecture",
        "refactor",
        "migrate",
        "migration",
        "debug",
        "race condition",
        "deadlock",
        "security",
        "vulnerability",
        "exploit",
        "performance",
        "optimize",
        "optimisation",
        "optimization",
        "concurrency",
        "distributed",
        "multi-file",
        "multifile",
        "codebase-wide",
        "root cause",
        "investigate",
        "production outage",
        "p0",
        "sev1",
        "design doc",
        "tradeoff",
        "trade-off",
        "formal proof",
        "prove correctness",
    ];
    HARD.iter().any(|k| lower.contains(k))
}

fn has_coding_signal(lower: &str) -> bool {
    const CODING: &[&str] = &[
        "implement",
        "fix",
        "bug",
        "pr ",
        "pull request",
        "unit test",
        "integration test",
        "write a test",
        "compile",
        "typecheck",
        "type error",
        "stack trace",
        "backtrace",
        "segfault",
        "panic",
        "diff",
        "patch",
        "commit",
        "merge conflict",
        "ci fail",
        "failing test",
        "flaky",
        "dockerfile",
        "kubernetes",
        "k8s",
        "sql",
        "regex",
        "async",
        "await",
        "lifetime",
        "borrow checker",
        "undefined behavior",
    ];
    CODING.iter().any(|k| lower.contains(k))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(
        preference: u8,
        floor: ReasoningEffort,
        ceiling: ReasoningEffort,
    ) -> EffortRouterConfig {
        EffortRouterConfig {
            enabled: true,
            preference,
            floor,
            ceiling,
        }
    }

    #[test]
    fn deserializes_effort_router_section() {
        let raw = r#"
            enabled = true
            preference = 3
            floor = "low"
            ceiling = "high"
        "#;
        let c: EffortRouterConfig = toml::from_str(raw).unwrap();
        assert!(c.enabled);
        assert_eq!(c.preference, 3);
        assert_eq!(c.floor, ReasoningEffort::Low);
        assert_eq!(c.ceiling, ReasoningEffort::High);
    }

    #[test]
    fn default_matches_ship_values() {
        let c = EffortRouterConfig::default();
        assert!(c.enabled);
        assert_eq!(c.preference, 3);
        assert_eq!(c.floor, ReasoningEffort::Low);
        assert_eq!(c.ceiling, ReasoningEffort::High);
    }

    #[test]
    fn simple_short_prompt_is_low() {
        let c = cfg(3, ReasoningEffort::Low, ReasoningEffort::High);
        assert_eq!(route_effort("hi", &c), ReasoningEffort::Low);
        assert_eq!(route_effort("thanks!", &c), ReasoningEffort::Low);
    }

    #[test]
    fn medium_neutral_prompt() {
        let c = cfg(3, ReasoningEffort::Low, ReasoningEffort::High);
        let prompt = "Summarize the main points of this paragraph about local weather patterns over the last week for a brief note.";
        assert_eq!(route_effort(prompt, &c), ReasoningEffort::Medium);
    }

    #[test]
    fn hard_coding_prompt_is_high() {
        let c = cfg(3, ReasoningEffort::Low, ReasoningEffort::High);
        let prompt = "Debug this race condition in the async worker pool and implement a fix with unit tests covering the deadlock.";
        assert_eq!(route_effort(prompt, &c), ReasoningEffort::High);
    }

    #[test]
    fn floor_and_ceiling_clamp() {
        let floor_med = cfg(1, ReasoningEffort::Medium, ReasoningEffort::High);
        assert_eq!(route_effort("hi", &floor_med), ReasoningEffort::Medium);

        let ceil_med = cfg(5, ReasoningEffort::Low, ReasoningEffort::Medium);
        let hard = "Architect a multi-file migration with security review and performance optimization.";
        assert_eq!(route_effort(hard, &ceil_med), ReasoningEffort::Medium);
    }

    #[test]
    fn preference_biases_rank() {
        let low_pref = cfg(1, ReasoningEffort::Low, ReasoningEffort::High);
        let high_pref = cfg(5, ReasoningEffort::Low, ReasoningEffort::High);
        let prompt = "Please explain how iterators work in Rust with a short example.";
        let a = route_effort(prompt, &low_pref);
        let b = route_effort(prompt, &high_pref);
        assert!(effort_rank(a) <= effort_rank(b));
        assert_eq!(b, ReasoningEffort::High);
    }

    #[test]
    fn maybe_route_respects_enabled_and_pin() {
        let mut c = EffortRouterConfig::default();
        assert!(maybe_route_effort("hi", &c, false).is_some());
        assert!(maybe_route_effort("hi", &c, true).is_none());
        c.enabled = false;
        assert!(maybe_route_effort("hi", &c, false).is_none());
    }

    #[test]
    fn inverted_floor_ceiling_swaps() {
        let c = cfg(3, ReasoningEffort::High, ReasoningEffort::Low);
        let (f, hi) = c.clamped_bounds();
        assert_eq!(f, ReasoningEffort::Low);
        assert_eq!(hi, ReasoningEffort::High);
    }

    #[test]
    fn triad_clamp_maps_extreme_catalog_levels() {
        assert_eq!(
            clamp_to_router_triad(ReasoningEffort::Xhigh),
            ReasoningEffort::High
        );
        assert_eq!(
            clamp_to_router_triad(ReasoningEffort::None),
            ReasoningEffort::Low
        );
    }

    #[test]
    fn full_config_deserializes_effort_router_section() {
        let raw = r#"
[effort_router]
enabled = false
preference = 1
floor = "medium"
ceiling = "high"
"#;
        let value: toml::Value = toml::from_str(raw).unwrap();
        let cfg = crate::agent::config::Config::new_from_toml_cfg(&value).unwrap();
        assert!(!cfg.effort_router.enabled);
        assert_eq!(cfg.effort_router.preference, 1);
        assert_eq!(cfg.effort_router.floor, ReasoningEffort::Medium);
        assert_eq!(cfg.effort_router.ceiling, ReasoningEffort::High);
    }

    #[test]
    fn full_config_defaults_effort_router_when_absent() {
        let empty = toml::Value::Table(toml::map::Map::new());
        let cfg = crate::agent::config::Config::new_from_toml_cfg(&empty).unwrap();
        assert_eq!(cfg.effort_router, EffortRouterConfig::default());
    }

    /// Precedence fixtures: pin wins; router when unpinned; disabled falls through.
    #[test]
    fn precedence_pin_over_router() {
        let c = EffortRouterConfig::default();
        assert_eq!(
            maybe_route_effort("Debug a race condition and implement a fix", &c, true),
            None,
            "pinned must not route"
        );
        assert_eq!(
            maybe_route_effort("Debug a race condition and implement a fix", &c, false),
            Some(ReasoningEffort::High)
        );
    }

    #[test]
    fn format_effort_status_marks_auto() {
        assert_eq!(
            format_effort_status(ReasoningEffort::Medium, true),
            "medium (auto)"
        );
        assert_eq!(
            format_effort_status(ReasoningEffort::High, false),
            "high"
        );
    }

    /// Full product precedence as a pure fixture (no ModelsManager):
    /// explicit pin > router > default_reasoning_effort > catalog high.
    #[test]
    fn precedence_pin_router_default_catalog_chain() {
        fn effective(
            prompt: &str,
            router: &EffortRouterConfig,
            pin: Option<ReasoningEffort>,
            default: Option<ReasoningEffort>,
            catalog: ReasoningEffort,
        ) -> ReasoningEffort {
            if let Some(p) = pin {
                return p;
            }
            if let Some(r) = maybe_route_effort(prompt, router, false) {
                return r;
            }
            default.unwrap_or(catalog)
        }

        let hard =
            "Debug this race condition in the async worker pool and implement a fix with unit tests.";
        let simple = "hi";
        let mut router = EffortRouterConfig::default();

        // Pin wins over a router that would choose high.
        assert_eq!(
            effective(
                hard,
                &router,
                Some(ReasoningEffort::Low),
                Some(ReasoningEffort::Medium),
                ReasoningEffort::High
            ),
            ReasoningEffort::Low
        );

        // Unpinned + enabled router chooses from the prompt (high for hard).
        assert_eq!(
            effective(hard, &router, None, Some(ReasoningEffort::Low), ReasoningEffort::High),
            ReasoningEffort::High
        );
        // Unpinned router chooses low for simple chat.
        assert_eq!(
            effective(simple, &router, None, Some(ReasoningEffort::High), ReasoningEffort::High),
            ReasoningEffort::Low
        );

        // Router disabled → fall through to default_reasoning_effort.
        router.enabled = false;
        assert_eq!(
            effective(
                hard,
                &router,
                None,
                Some(ReasoningEffort::Medium),
                ReasoningEffort::High
            ),
            ReasoningEffort::Medium
        );

        // No pin, router off, no default → catalog high.
        assert_eq!(
            effective(hard, &router, None, None, ReasoningEffort::High),
            ReasoningEffort::High
        );
    }

    #[test]
    fn effort_auto_meta_detects_token() {
        let mut m = serde_json::Map::new();
        m.insert(
            xai_grok_sampling_types::REASONING_EFFORT_META_KEY.into(),
            serde_json::Value::String("auto".into()),
        );
        assert!(is_effort_auto_meta(Some(&m)));
        m.insert(
            xai_grok_sampling_types::REASONING_EFFORT_META_KEY.into(),
            serde_json::Value::String("high".into()),
        );
        assert!(!is_effort_auto_meta(Some(&m)));
        assert!(!is_effort_auto_meta(None));
    }
}
