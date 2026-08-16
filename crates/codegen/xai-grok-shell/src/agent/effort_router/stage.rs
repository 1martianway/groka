//! Switchyard-style stage scorer over last-turn tool outcomes.
//!
//! WRONG (errors / spin / explore) pushes toward high effort.
//! PROGRESS (writes landing, no failures) pushes toward low/medium.
//! One full signal scores ~0.46 after tanh; two corroborating signals
//! clear the default 0.5 threshold.

use super::{DecisionSource, StageVerdict};
use crate::session::signals::ToolOutcome;
use xai_grok_sampling_types::ReasoningEffort;

/// Axes the cascade reads. All in `[0, 1]` except the boolean overrides.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StageSignals {
    pub severity: f32,
    pub spinning: f32,
    pub exploring: f32,
    pub production: f32,
    pub critical: bool,
    pub tests_passed: bool,
}

fn is_write_tool(name: &str) -> bool {
    matches!(
        name,
        "apply_patch"
            | "search_replace"
            | "str_replace"
            | "str_replace_editor"
            | "write_file"
            | "edit_file"
            | "create_file"
    )
}

fn is_explore_tool(name: &str) -> bool {
    matches!(
        name,
        "read_file"
            | "grep"
            | "codebase_search"
            | "glob"
            | "list_dir"
            | "list_directory"
            | "search_codebase"
    )
}

fn is_shell_tool(name: &str) -> bool {
    matches!(name, "bash" | "shell" | "run_command")
}

/// Fold last-turn tool outcomes + a doom-loop flag into stage axes.
pub fn collect_stage_signals(outcomes: &[ToolOutcome], doom_fired: bool) -> StageSignals {
    if outcomes.is_empty() && !doom_fired {
        return StageSignals::default();
    }

    let mut writes = 0u32;
    let mut write_ok = 0u32;
    let mut explores = 0u32;
    let mut shells = 0u32;
    let mut shell_fail = 0u32;
    let mut fail = 0u32;
    let mut total = 0u32;

    for o in outcomes {
        let n = o.successes.saturating_add(o.failures);
        total = total.saturating_add(n);
        fail = fail.saturating_add(o.failures);
        if is_write_tool(&o.tool_name) {
            writes = writes.saturating_add(n);
            write_ok = write_ok.saturating_add(o.successes);
        } else if is_explore_tool(&o.tool_name) {
            explores = explores.saturating_add(n);
        } else if is_shell_tool(&o.tool_name) {
            shells = shells.saturating_add(n);
            shell_fail = shell_fail.saturating_add(o.failures);
        }
    }

    let fail_ratio = if total == 0 {
        0.0
    } else {
        fail as f32 / total as f32
    };
    let severity = if doom_fired {
        1.0
    } else {
        (fail_ratio * 1.5).clamp(0.0, 1.0)
    };

    let spinning = if total >= 4 && writes == 0 && fail > 0 {
        1.0
    } else if total >= 3 && writes == 0 {
        0.6
    } else {
        0.0
    };

    let exploring = if explores > 0 && writes == 0 {
        (explores as f32 / total.max(1) as f32).clamp(0.0, 1.0)
    } else {
        0.0
    };

    let production = if writes == 0 {
        0.0
    } else {
        (write_ok as f32 / writes as f32).clamp(0.0, 1.0)
    };

    let tests_passed = write_ok > 0 && fail == 0 && shell_fail == 0 && shells > 0;
    let critical = doom_fired || (shell_fail > 0 && writes == 0 && fail_ratio >= 0.5);

    StageSignals {
        severity,
        spinning,
        exploring,
        production,
        critical,
        tests_passed,
    }
}

/// Score stage signals. `None` when there is nothing to act on.
///
/// `threshold` is in `[0, 1]` (default 0.5).
pub fn score_stage(signals: &StageSignals, threshold: f32) -> Option<StageVerdict> {
    if signals.critical {
        return Some(StageVerdict {
            effort: ReasoningEffort::High,
            confidence: 1.0,
            source: DecisionSource::Override,
        });
    }
    if signals.tests_passed && signals.production >= 0.5 {
        return Some(StageVerdict {
            effort: ReasoningEffort::Low,
            confidence: 0.85,
            source: DecisionSource::TestsPassed,
        });
    }

    let empty = signals.severity == 0.0
        && signals.spinning == 0.0
        && signals.exploring == 0.0
        && signals.production == 0.0;
    if empty {
        return None;
    }

    // One full axis contributes 0.5 so tanh(0.5) ≈ 0.46 — below the default
    // 0.5 bar. Two corroborating axes clear it.
    let wrong = (signals.severity + signals.spinning + signals.exploring).min(2.0) * 0.5;
    let right = signals.production * 0.5;
    let signed = wrong - right;
    let confidence = signed.abs().tanh();
    if confidence < threshold {
        return None;
    }
    let effort = if signed > 0.0 {
        ReasoningEffort::High
    } else {
        ReasoningEffort::Low
    };
    Some(StageVerdict {
        effort,
        confidence,
        source: DecisionSource::Stage,
    })
}

/// Last turn counts as a strike when doom-loop fired or a write/shell tool failed.
pub fn turn_was_bad(outcomes: &[ToolOutcome], doom_fired: bool) -> bool {
    if doom_fired {
        return true;
    }
    outcomes.iter().any(|o| {
        o.failures > 0 && (is_write_tool(&o.tool_name) || is_shell_tool(&o.tool_name))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn outcome(name: &str, ok: u32, fail: u32) -> ToolOutcome {
        ToolOutcome {
            tool_name: name.to_string(),
            successes: ok,
            failures: fail,
        }
    }

    #[test]
    fn empty_outcomes_score_none() {
        let s = collect_stage_signals(&[], false);
        assert_eq!(s, StageSignals::default());
        assert!(score_stage(&s, 0.5).is_none());
    }

    #[test]
    fn one_signal_stays_under_half() {
        let s = collect_stage_signals(&[outcome("read_file", 2, 0)], false);
        assert!(s.exploring > 0.0);
        assert!(score_stage(&s, 0.5).is_none(), "one explore signal must stay under 0.5");
    }

    #[test]
    fn error_plus_spinning_is_high() {
        let s = collect_stage_signals(
            &[
                outcome("bash", 0, 2),
                outcome("read_file", 3, 0),
                outcome("grep", 2, 0),
            ],
            false,
        );
        let v = score_stage(&s, 0.5).expect("two WRONG signals");
        assert_eq!(v.effort, ReasoningEffort::High);
        assert!(matches!(
            v.source,
            DecisionSource::Stage | DecisionSource::Override
        ));
    }

    #[test]
    fn writes_and_passing_shell_are_low() {
        let s = collect_stage_signals(
            &[outcome("apply_patch", 2, 0), outcome("bash", 1, 0)],
            false,
        );
        assert!(s.tests_passed);
        let v = score_stage(&s, 0.5).expect("tests_passed");
        assert_eq!(v.effort, ReasoningEffort::Low);
        assert_eq!(v.source, DecisionSource::TestsPassed);
    }

    #[test]
    fn doom_loop_is_critical_override() {
        let s = collect_stage_signals(&[outcome("read_file", 1, 0)], true);
        assert!(s.critical);
        let v = score_stage(&s, 0.5).unwrap();
        assert_eq!(v.source, DecisionSource::Override);
        assert_eq!(v.effort, ReasoningEffort::High);
    }

    #[test]
    fn turn_was_bad_only_on_write_or_shell_failure() {
        assert!(!turn_was_bad(&[outcome("grep", 0, 1)], false));
        assert!(turn_was_bad(&[outcome("bash", 0, 1)], false));
        assert!(turn_was_bad(&[outcome("apply_patch", 0, 1)], false));
        assert!(turn_was_bad(&[], true));
    }
}
