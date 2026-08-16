//! Per-turn effort router.
//!
//! When `[effort_router]` is enabled and effort is not pinned (CLI `/effort` /
//! `--effort` / persona), choose `low` | `medium` | `high` and stamp
//! `sampling_config.reasoning_effort`. Never swaps the model.
//!
//! Default `mode = "hybrid"` is a Switchyard-style cascade:
//! escalation → stage signals → obvious heuristic → optional LLM judge
//! → heuristic fall-open. `mode = "heuristic"` is the original v0
//! keyword/length scorer (kept as the cheap leaf and as a config escape).
//!
//! Precedence (callers enforce pin):
//! explicit pin > router > `default_reasoning_effort` > catalog high.

mod classifier;
mod stage;

pub use classifier::{
    build_classifier_request, classify_with_timeout, parse_classifier_verdict,
};
pub use stage::{collect_stage_signals, score_stage, turn_was_bad, StageSignals};

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

/// How the router picks effort.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RouterMode {
    /// Stage + obvious heuristic + judge on remaining ambiguity.
    #[default]
    Hybrid,
    /// v0 keyword/length scorer only.
    Heuristic,
    /// Always consult the judge (still fail-open to the heuristic).
    Classifier,
}

impl RouterMode {
    pub fn allows_classifier(self) -> bool {
        matches!(self, Self::Hybrid | Self::Classifier)
    }
}

/// `[effort_router]` in config.toml.
///
/// ```toml
/// [effort_router]
/// enabled = true
/// preference = 3   # 1..=5; 3 is neutral, higher biases toward high effort
/// floor = "low"
/// ceiling = "high"
/// mode = "hybrid"              # heuristic | hybrid | classifier
/// confidence_threshold = 50    # 0..=100; 50 ≈ Switchyard 0.5
/// escalation_strikes = 2
/// classifier_timeout_ms = 500
/// recent_window = 3
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
    #[serde(default)]
    pub mode: RouterMode,
    /// 0..=100; 50 means 0.5. Stage confidence must meet this to fire.
    #[serde(default = "default_confidence_threshold")]
    pub confidence_threshold: u8,
    /// Consecutive bad turns before the session is pinned high.
    #[serde(default = "default_escalation_strikes")]
    pub escalation_strikes: u8,
    #[serde(default = "default_classifier_timeout_ms")]
    pub classifier_timeout_ms: u16,
    #[serde(default = "default_recent_window")]
    pub recent_window: u8,
}

fn default_floor() -> ReasoningEffort {
    ReasoningEffort::Low
}

fn default_ceiling() -> ReasoningEffort {
    ReasoningEffort::High
}

fn default_confidence_threshold() -> u8 {
    50
}

fn default_escalation_strikes() -> u8 {
    2
}

fn default_classifier_timeout_ms() -> u16 {
    500
}

fn default_recent_window() -> u8 {
    3
}

impl Default for EffortRouterConfig {
    fn default() -> Self {
        Self {
            // Fork default: auto-route instead of always shipping catalog high.
            enabled: true,
            preference: 3,
            floor: ReasoningEffort::Low,
            ceiling: ReasoningEffort::High,
            mode: RouterMode::Hybrid,
            confidence_threshold: default_confidence_threshold(),
            escalation_strikes: default_escalation_strikes(),
            classifier_timeout_ms: default_classifier_timeout_ms(),
            recent_window: default_recent_window(),
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

    /// Stage confidence bar in `[0, 1]`.
    pub fn stage_threshold(&self) -> f32 {
        (self.confidence_threshold.min(100) as f32) / 100.0
    }

    pub fn classifier_timeout(&self) -> std::time::Duration {
        std::time::Duration::from_millis(self.classifier_timeout_ms as u64)
    }
}

/// Why the router picked this effort.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecisionSource {
    Escalation,
    Override,
    TestsPassed,
    Stage,
    Heuristic,
    Classifier,
    FallOpen,
}

impl DecisionSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Escalation => "escalation",
            Self::Override => "override",
            Self::TestsPassed => "tests_passed",
            Self::Stage => "stage",
            Self::Heuristic => "heuristic",
            Self::Classifier => "classifier",
            Self::FallOpen => "fall_open",
        }
    }
}

/// Stage scorer output.
#[derive(Clone, Debug, PartialEq)]
pub struct StageVerdict {
    pub effort: ReasoningEffort,
    pub confidence: f32,
    pub source: DecisionSource,
}

/// Judge output.
#[derive(Clone, Debug, PartialEq)]
pub struct ClassifierVerdict {
    pub effort: ReasoningEffort,
    pub confidence: f32,
    pub reason: String,
}

/// Final routing decision.
#[derive(Clone, Debug, PartialEq)]
pub struct RouteDecision {
    pub effort: ReasoningEffort,
    pub source: DecisionSource,
    pub confidence: f32,
    pub reason: String,
}

impl RouteDecision {
    fn new(
        effort: ReasoningEffort,
        source: DecisionSource,
        confidence: f32,
        reason: impl Into<String>,
        cfg: &EffortRouterConfig,
    ) -> Self {
        let (floor, ceiling) = cfg.clamped_bounds();
        let rank = effort_rank(effort) as i8;
        Self {
            effort: clamp_rank_to_bounds(rank, floor, ceiling),
            source,
            confidence,
            reason: reason.into(),
        }
    }
}

/// What [`route_cascade`] still needs from the caller.
#[derive(Clone, Debug, PartialEq)]
pub enum CascadeStep {
    Done(RouteDecision),
    /// Hybrid/classifier mode, prompt is ambiguous — run the judge.
    NeedClassifier,
}

/// Obvious-enough heuristic: greeting-short → low, hard+coding → high.
pub fn heuristic_is_obvious(prompt: &str) -> Option<ReasoningEffort> {
    let trimmed = prompt.trim();
    let char_len = trimmed.chars().count();
    let lower = trimmed.to_ascii_lowercase();
    if has_simple_signal(&lower, char_len) && char_len < 40 {
        return Some(ReasoningEffort::Low);
    }
    if has_hard_signal(&lower) && has_coding_signal(&lower) {
        return Some(ReasoningEffort::High);
    }
    None
}

/// Sync half of the cascade. Callers that get [`CascadeStep::NeedClassifier`]
/// run the judge and finish with [`finish_cascade`].
pub fn route_cascade(
    prompt: &str,
    cfg: &EffortRouterConfig,
    pinned: bool,
    escalated: bool,
    stage: Option<StageVerdict>,
) -> Option<CascadeStep> {
    if !cfg.enabled || pinned {
        return None;
    }
    if escalated {
        return Some(CascadeStep::Done(RouteDecision::new(
            ReasoningEffort::High,
            DecisionSource::Escalation,
            1.0,
            "session escalated after repeated bad turns",
            cfg,
        )));
    }
    if cfg.mode != RouterMode::Classifier
        && let Some(stage) = stage
    {
        return Some(CascadeStep::Done(RouteDecision::new(
            stage.effort,
            stage.source,
            stage.confidence,
            format!("stage {}", stage.source.as_str()),
            cfg,
        )));
    }
    if cfg.mode == RouterMode::Heuristic {
        return Some(CascadeStep::Done(heuristic_decision(prompt, cfg, DecisionSource::Heuristic)));
    }
    if cfg.mode == RouterMode::Hybrid
        && let Some(effort) = heuristic_is_obvious(prompt)
    {
        return Some(CascadeStep::Done(RouteDecision::new(
            effort,
            DecisionSource::Heuristic,
            0.8,
            "obvious heuristic",
            cfg,
        )));
    }
    if cfg.mode.allows_classifier() {
        return Some(CascadeStep::NeedClassifier);
    }
    Some(CascadeStep::Done(heuristic_decision(
        prompt,
        cfg,
        DecisionSource::FallOpen,
    )))
}

/// Apply a judge verdict, or fall open to the heuristic.
pub fn finish_cascade(
    prompt: &str,
    cfg: &EffortRouterConfig,
    verdict: Option<ClassifierVerdict>,
) -> RouteDecision {
    if let Some(v) = verdict {
        return RouteDecision::new(
            v.effort,
            DecisionSource::Classifier,
            v.confidence,
            if v.reason.is_empty() {
                "classifier".to_string()
            } else {
                v.reason
            },
            cfg,
        );
    }
    heuristic_decision(prompt, cfg, DecisionSource::FallOpen)
}

fn heuristic_decision(
    prompt: &str,
    cfg: &EffortRouterConfig,
    source: DecisionSource,
) -> RouteDecision {
    RouteDecision::new(route_effort(prompt, cfg), source, 0.4, source.as_str(), cfg)
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
            ..EffortRouterConfig::default()
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
        assert_eq!(c.mode, RouterMode::Hybrid);
        assert_eq!(c.confidence_threshold, 50);
        assert_eq!(c.escalation_strikes, 2);
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

    #[test]
    fn cascade_pin_and_disabled_return_none() {
        let mut c = EffortRouterConfig::default();
        assert!(route_cascade("hi", &c, true, false, None).is_none());
        c.enabled = false;
        assert!(route_cascade("hi", &c, false, false, None).is_none());
    }

    #[test]
    fn cascade_escalated_ignores_hi() {
        let c = EffortRouterConfig::default();
        match route_cascade("hi", &c, false, true, None) {
            Some(CascadeStep::Done(d)) => {
                assert_eq!(d.effort, ReasoningEffort::High);
                assert_eq!(d.source, DecisionSource::Escalation);
            }
            other => panic!("expected escalation, got {other:?}"),
        }
    }

    #[test]
    fn cascade_hybrid_obvious_skips_classifier() {
        let c = EffortRouterConfig::default();
        match route_cascade("hi", &c, false, false, None) {
            Some(CascadeStep::Done(d)) => {
                assert_eq!(d.effort, ReasoningEffort::Low);
                assert_eq!(d.source, DecisionSource::Heuristic);
            }
            other => panic!("expected obvious heuristic, got {other:?}"),
        }
        let hard = "Debug this race condition and implement a fix with unit tests.";
        match route_cascade(hard, &c, false, false, None) {
            Some(CascadeStep::Done(d)) => {
                assert_eq!(d.effort, ReasoningEffort::High);
                assert_eq!(d.source, DecisionSource::Heuristic);
            }
            other => panic!("expected obvious high, got {other:?}"),
        }
    }

    #[test]
    fn cascade_ambiguous_asks_for_classifier() {
        let c = EffortRouterConfig::default();
        let prompt = "Can you take a look at this and tell me what you think?";
        match route_cascade(prompt, &c, false, false, None) {
            Some(CascadeStep::NeedClassifier) => {}
            other => panic!("expected NeedClassifier, got {other:?}"),
        }
    }

    #[test]
    fn cascade_heuristic_mode_never_asks_classifier() {
        let mut c = EffortRouterConfig::default();
        c.mode = RouterMode::Heuristic;
        let prompt = "Can you take a look at this and tell me what you think?";
        match route_cascade(prompt, &c, false, false, None) {
            Some(CascadeStep::Done(d)) => {
                assert_eq!(d.source, DecisionSource::Heuristic);
            }
            other => panic!("expected heuristic Done, got {other:?}"),
        }
    }

    #[test]
    fn finish_cascade_timeout_falls_open() {
        let c = EffortRouterConfig::default();
        let d = finish_cascade("Summarize the last week's weather in two sentences please.", &c, None);
        assert_eq!(d.source, DecisionSource::FallOpen);
        assert_eq!(d.effort, route_effort("Summarize the last week's weather in two sentences please.", &c));
    }

    #[test]
    fn finish_cascade_uses_verdict() {
        let c = EffortRouterConfig::default();
        let d = finish_cascade(
            "anything",
            &c,
            Some(ClassifierVerdict {
                effort: ReasoningEffort::High,
                confidence: 0.91,
                reason: "underspecified architecture".into(),
            }),
        );
        assert_eq!(d.source, DecisionSource::Classifier);
        assert_eq!(d.effort, ReasoningEffort::High);
        assert_eq!(d.reason, "underspecified architecture");
    }

    #[test]
    fn old_toml_without_new_keys_still_deserializes() {
        let raw = r#"
            enabled = true
            preference = 3
            floor = "low"
            ceiling = "high"
        "#;
        let c: EffortRouterConfig = toml::from_str(raw).unwrap();
        assert_eq!(c.mode, RouterMode::Hybrid);
        assert_eq!(c.confidence_threshold, 50);
        assert_eq!(c.escalation_strikes, 2);
    }
}
