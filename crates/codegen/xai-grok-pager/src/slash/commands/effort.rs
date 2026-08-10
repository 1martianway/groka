//! `/effort` — set reasoning effort on the current model without re-picking it.
//!
//! Thin wrapper over `Action::SwitchModel` with the session's current model
//! id and the chosen effort (same wire path as `/model <name> <effort>`).
//!
//! `/effort auto` re-enables the per-turn effort router (unpins).

use crate::app::actions::Action;
use crate::slash::command::{AppCtx, ArgItem, CommandExecCtx, CommandResult, SlashCommand};
use crate::slash::commands::effort_levels::build_effort_arg_items;

/// Set reasoning effort for the active model.
pub struct EffortCommand;

impl SlashCommand for EffortCommand {
    fn name(&self) -> &str {
        "effort"
    }

    fn description(&self) -> &str {
        "Set reasoning effort for the current model (or auto)"
    }

    fn session_scoped(&self) -> bool {
        true
    }

    fn usage(&self) -> &str {
        // Levels are model-specific; empty-args and UnknownToken errors list
        // the active model's offered option ids instead of a hardcoded set.
        // `auto` re-enables the per-turn router.
        "/effort <level|auto>"
    }

    fn takes_args(&self) -> bool {
        true
    }

    fn args_required(&self) -> bool {
        true
    }

    fn arg_placeholder(&self) -> Option<&str> {
        Some("<level|auto>")
    }

    fn suggest_args(&self, ctx: &AppCtx, _args_query: &str) -> Option<Vec<ArgItem>> {
        let options = ctx.models.reasoning_effort_options();
        if options.is_empty() {
            // Still offer auto so users can re-enable the router.
            return Some(vec![ArgItem {
                display: "auto".into(),
                insert_text: "auto".into(),
                match_text: "a auto".into(),
                description: "Re-enable per-turn effort router".into(),
            }]);
        }
        let mut items = build_effort_arg_items(
            &options,
            ctx.models.reasoning_effort,
            true,
            |option| option.id.clone(),
        );
        items.push(ArgItem {
            display: if ctx.models.effort_auto {
                "auto (active)".into()
            } else {
                "auto".into()
            },
            insert_text: "auto".into(),
            match_text: "z auto".into(),
            description: "Re-enable per-turn effort router".into(),
        });
        Some(items)
    }

    fn run(&self, ctx: &mut CommandExecCtx, args: &str) -> CommandResult {
        let trimmed = args.trim();
        let Some(model_id) = ctx.models.current.clone() else {
            return CommandResult::Error("No active model".into());
        };

        if trimmed.is_empty() {
            let offered: Vec<String> = ctx
                .models
                .reasoning_effort_options_for(&model_id)
                .into_iter()
                .map(|opt| opt.id)
                .collect();
            let current = if ctx.models.effort_auto {
                " (current: auto)".to_string()
            } else {
                ctx.models
                    .reasoning_effort
                    .map(|e| format!(" (current: {e})"))
                    .unwrap_or_default()
            };
            let levels = if offered.is_empty() {
                "level|auto".to_string()
            } else {
                format!("{}|auto", offered.join("|"))
            };
            return CommandResult::Error(format!("Usage: /effort <{levels}>{current}"));
        }

        if trimmed.eq_ignore_ascii_case("auto") {
            return CommandResult::Action(Action::EffortAuto { model_id });
        }

        // Same gate-first policy as the CLI (`--effort`) and headless.
        match ctx.models.resolve_effort_for_model(&model_id, trimmed) {
            Ok(effort) => CommandResult::Action(Action::SwitchModel {
                model_id,
                effort: Some(effort),
            }),
            Err(err) => CommandResult::Error(err.message()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::acp::model_state::ModelState;
    use crate::slash::commands::effort_levels::EFFORT_LEVELS;
    use agent_client_protocol as acp;
    use std::sync::Arc;
    use xai_grok_shell::sampling::types::ReasoningEffort;

    fn model_with_reasoning(id: &str, name: &str) -> (acp::ModelId, acp::ModelInfo) {
        let id = acp::ModelId::new(Arc::from(id));
        let mut meta = serde_json::Map::new();
        meta.insert(
            "supportsReasoningEffort".into(),
            serde_json::Value::Bool(true),
        );
        let info = acp::ModelInfo::new(id.clone(), name.to_string())
            .meta(serde_json::Value::Object(meta).as_object().cloned());
        (id, info)
    }

    fn plain_model(id: &str, name: &str) -> (acp::ModelId, acp::ModelInfo) {
        let id = acp::ModelId::new(Arc::from(id));
        let info = acp::ModelInfo::new(id.clone(), name.to_string());
        (id, info)
    }

    static EMPTY_BUNDLE: crate::app::bundle::BundleState = crate::app::bundle::BundleState {
        has_cache: false,
        version: String::new(),
        personas: Vec::new(),
        roles: Vec::new(),
        agents: Vec::new(),
        skills: Vec::new(),
        persona_details: Vec::new(),
        role_details: Vec::new(),
    };

    fn dummy_exec_ctx(models: &ModelState) -> CommandExecCtx<'_> {
        CommandExecCtx {
            models,
            session_id: None,
            bundle_state: &EMPTY_BUNDLE,
            screen_mode: crate::app::ScreenMode::Inline,
            billing_surface_visible: true,
            usage_command_visible: true,
            pager_state: crate::settings::PagerLocalSnapshot {
                multiline_mode: false,
                yolo_mode: false,
                ..crate::settings::PagerLocalSnapshot::default()
            },
        }
    }

    #[test]
    fn empty_args_errors_with_usage() {
        let mut state = ModelState::default();
        let (id, info) = model_with_reasoning("reasoning-x", "Reasoning X");
        state.available.insert(id.clone(), info);
        state.current = Some(id);
        state.reasoning_effort = Some(ReasoningEffort::Medium);
        let mut ctx = dummy_exec_ctx(&state);
        let result = EffortCommand.run(&mut ctx, "");
        match result {
            CommandResult::Error(msg) => {
                assert!(msg.contains("Usage: /effort"));
                // Legacy menu option ids only — not none/minimal; always includes auto.
                assert!(msg.contains("xhigh|high|medium|low|auto"), "msg={msg}");
                assert!(msg.contains("current: medium"));
                assert!(!msg.contains("none"));
                assert!(!msg.contains("minimal"));
            }
            other => panic!("expected Error, got {other:?}"),
        }
    }

    #[test]
    fn unknown_level_errors() {
        let mut state = ModelState::default();
        let (id, info) = model_with_reasoning("reasoning-x", "Reasoning X");
        state.available.insert(id.clone(), info);
        state.current = Some(id);
        let mut ctx = dummy_exec_ctx(&state);
        let result = EffortCommand.run(&mut ctx, "turbo");
        match result {
            CommandResult::Error(msg) => {
                assert!(msg.contains("unknown effort level 'turbo'"), "msg={msg}");
                assert!(msg.contains("use one of:"), "msg={msg}");
                assert!(msg.contains("xhigh"), "msg={msg}");
                assert!(!msg.contains("none"), "msg={msg}");
                assert!(!msg.contains("minimal"), "msg={msg}");
            }
            other => panic!("expected Error, got {other:?}"),
        }
    }

    #[test]
    fn valid_level_dispatches_switch_model_on_current() {
        let mut state = ModelState::default();
        let (id, info) = model_with_reasoning("reasoning-x", "Reasoning X");
        state.available.insert(id.clone(), info);
        state.current = Some(id.clone());
        let mut ctx = dummy_exec_ctx(&state);
        let result = EffortCommand.run(&mut ctx, "high");
        match result {
            CommandResult::Action(Action::SwitchModel { model_id, effort }) => {
                assert_eq!(model_id, id);
                assert_eq!(effort, Some(ReasoningEffort::High));
            }
            other => panic!("expected SwitchModel with effort, got {other:?}"),
        }
    }

    #[test]
    fn auto_dispatches_effort_auto_on_current() {
        let mut state = ModelState::default();
        let (id, info) = model_with_reasoning("reasoning-x", "Reasoning X");
        state.available.insert(id.clone(), info);
        state.current = Some(id.clone());
        let mut ctx = dummy_exec_ctx(&state);
        match EffortCommand.run(&mut ctx, "auto") {
            CommandResult::Action(Action::EffortAuto { model_id }) => {
                assert_eq!(model_id, id);
            }
            other => panic!("expected EffortAuto, got {other:?}"),
        }
        match EffortCommand.run(&mut ctx, "AUTO") {
            CommandResult::Action(Action::EffortAuto { .. }) => {}
            other => panic!("expected case-insensitive EffortAuto, got {other:?}"),
        }
    }

    #[test]
    fn effort_status_label_marks_auto() {
        let mut state = ModelState::default();
        state.reasoning_effort = Some(ReasoningEffort::Medium);
        assert_eq!(state.effort_status_label().as_deref(), Some("medium"));
        state.effort_auto = true;
        assert_eq!(
            state.effort_status_label().as_deref(),
            Some("medium (auto)")
        );
    }

    #[test]
    fn none_and_minimal_rejected_when_model_menu_omits_them() {
        // Legacy fallback menu is low..xhigh — `none`/`minimal` used to pass
        // through and 400 on grok-4.5; reject at the TUI instead.
        let mut state = ModelState::default();
        let (id, info) = model_with_reasoning("reasoning-x", "Reasoning X");
        state.available.insert(id.clone(), info);
        state.current = Some(id);
        let mut ctx = dummy_exec_ctx(&state);
        for token in ["none", "minimal"] {
            let result = EffortCommand.run(&mut ctx, token);
            match result {
                CommandResult::Error(ref msg) => {
                    assert!(
                        msg.contains(&format!("unknown effort level '{token}'")),
                        "expected Error for {token}, got {msg}"
                    );
                    // Must not re-advertise the rejected token as a valid choice
                    // (aside from quoting it in "unknown effort level '…'").
                    let after_prefix = msg
                        .split_once("; ")
                        .map(|(_, rest)| rest)
                        .unwrap_or(msg.as_str());
                    assert!(
                        !after_prefix.contains(token),
                        "error must not list {token} as offered: {msg}"
                    );
                    assert!(!msg.contains("unset"), "msg={msg}");
                }
                other => panic!("expected Error for {token}, got {other:?}"),
            }
        }
    }

    #[test]
    fn none_accepted_when_model_menu_offers_it() {
        let mut state = ModelState::default();
        let id = acp::ModelId::new(Arc::from("voice-dual"));
        let info = acp::ModelInfo::new(id.clone(), "Voice Dual".to_string()).meta(
            serde_json::json!({
                "supportsReasoningEffort": true,
                "reasoningEfforts": [
                    { "value": "none", "label": "None", "default": true },
                    { "value": "high", "label": "High" },
                ],
            })
            .as_object()
            .cloned(),
        );
        state.available.insert(id.clone(), info);
        state.current = Some(id.clone());
        let mut ctx = dummy_exec_ctx(&state);
        let result = EffortCommand.run(&mut ctx, "none");
        match result {
            CommandResult::Action(Action::SwitchModel { model_id, effort }) => {
                assert_eq!(model_id, id);
                assert_eq!(effort, Some(ReasoningEffort::None));
            }
            other => panic!("expected SwitchModel with none, got {other:?}"),
        }
    }

    #[test]
    fn remap_id_dispatches_mapped_canonical_effort() {
        let mut state = ModelState::default();
        let id = acp::ModelId::new(Arc::from("reasoning-x"));
        let info = acp::ModelInfo::new(id.clone(), "Reasoning X".to_string()).meta(
            serde_json::json!({
                "supportsReasoningEffort": true,
                "reasoningEfforts": [{ "id": "deep", "value": "xhigh", "label": "Deep" }],
            })
            .as_object()
            .cloned(),
        );
        state.available.insert(id.clone(), info);
        state.current = Some(id.clone());
        let mut ctx = dummy_exec_ctx(&state);
        // The rendered row inserts the id; `/effort deep` must send `xhigh`.
        match EffortCommand.run(&mut ctx, "deep") {
            CommandResult::Action(Action::SwitchModel { model_id, effort }) => {
                assert_eq!(model_id, id);
                assert_eq!(effort, Some(ReasoningEffort::Xhigh));
            }
            other => panic!("expected SwitchModel with remapped effort, got {other:?}"),
        }
    }

    #[test]
    fn non_reasoning_model_errors() {
        let mut state = ModelState::default();
        let (id, info) = plain_model("grok-4.5", "Grok 4.5");
        state.available.insert(id.clone(), info);
        state.current = Some(id);
        let mut ctx = dummy_exec_ctx(&state);
        let result = EffortCommand.run(&mut ctx, "high");
        assert!(matches!(
            result,
            CommandResult::Error(msg) if msg.contains("does not support reasoning effort")
        ));
    }

    #[test]
    fn no_current_model_errors() {
        let mut state = ModelState::default();
        let (id, info) = model_with_reasoning("reasoning-x", "Reasoning X");
        state.available.insert(id, info);
        let mut ctx = dummy_exec_ctx(&state);
        let result = EffortCommand.run(&mut ctx, "high");
        assert!(matches!(result, CommandResult::Error(msg) if msg.contains("No active model")));
    }

    #[test]
    fn suggest_args_offers_auto_without_current_or_support() {
        let cmd = EffortCommand;
        let empty = ModelState::default();
        let ctx = AppCtx {
            models: &empty,
            cwd: std::path::Path::new("."),
            has_session_announcements: false,
            billing_surface_visible: true,
            usage_command_visible: true,
            workflows_available: true,
            screen_mode: crate::app::ScreenMode::Fullscreen,
        };
        let items = cmd.suggest_args(&ctx, "").unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].insert_text, "auto");

        let mut plain = ModelState::default();
        let (id, info) = plain_model("grok-4.5", "Grok 4.5");
        plain.available.insert(id.clone(), info);
        plain.current = Some(id);
        let ctx = AppCtx {
            models: &plain,
            cwd: std::path::Path::new("."),
            has_session_announcements: false,
            billing_surface_visible: true,
            usage_command_visible: true,
            workflows_available: true,
            screen_mode: crate::app::ScreenMode::Fullscreen,
        };
        let items = cmd.suggest_args(&ctx, "").unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].insert_text, "auto");
    }

    #[test]
    fn suggest_args_lists_levels_with_active_marker() {
        let mut state = ModelState::default();
        let (id, info) = model_with_reasoning("reasoning-x", "Reasoning X");
        state.available.insert(id.clone(), info);
        state.current = Some(id);
        state.reasoning_effort = Some(ReasoningEffort::High);

        let cmd = EffortCommand;
        let ctx = AppCtx {
            models: &state,
            cwd: std::path::Path::new("."),
            has_session_announcements: false,
            billing_surface_visible: true,
            usage_command_visible: true,
            workflows_available: true,
            screen_mode: crate::app::ScreenMode::Fullscreen,
        };
        let items = cmd.suggest_args(&ctx, "").unwrap();
        // Legacy levels + trailing `auto`.
        assert_eq!(items.len(), EFFORT_LEVELS.len() + 1);
        assert_eq!(items[0].insert_text, "xhigh");
        assert_eq!(items[1].insert_text, "high");
        assert_eq!(items[1].display, "high (active)");
        assert_eq!(items[2].insert_text, "medium");
        assert_eq!(items[3].insert_text, "low");
        assert_eq!(items[4].insert_text, "auto");
        assert!(items[0].match_text.starts_with("a "));
        assert!(items[3].match_text.starts_with("d "));
    }
}
