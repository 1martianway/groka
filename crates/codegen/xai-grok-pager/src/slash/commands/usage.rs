//! `/usage` — session token/cost; consumer accounts can also manage billing
//! and toggle the prompt chrome limit bar.
//!
//! External-auth deployments (`auth_provider_command`) never reach grok.com
//! billing, so the command is hidden and refused via
//! [`AppCtx::usage_command_visible`].
//!
//! Subcommands:
//! - `show` / bare — open the usage summary
//! - `manage` — billing (consumer only)
//! - `bar` / `bar on` / `bar off` — toggle the weekly/period limit bar on the
//!   prompt chrome (default on; persists to `[ui].show_limit_bar`)

use crate::app::actions::Action;
use crate::slash::command::{AppCtx, ArgItem, CommandExecCtx, CommandResult, SlashCommand};
use agent_client_protocol as acp;

pub struct UsageCommand;

/// Detect external-auth installs once at pager startup.
pub(crate) fn detect_external_auth_provider(auth_methods: &[acp::AuthMethod]) -> bool {
    auth_methods.iter().any(auth_method_is_external_provider)
        || auth_provider_env_set()
        || auth_provider_config_set()
}

fn auth_method_is_external_provider(method: &acp::AuthMethod) -> bool {
    method
        .meta()
        .as_ref()
        .and_then(|v| v.get("external_provider"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

fn auth_provider_env_set() -> bool {
    std::env::var("GROK_AUTH_PROVIDER_COMMAND")
        .ok()
        .is_some_and(|s| !s.trim().is_empty())
}

fn auth_provider_config_set() -> bool {
    let Ok(raw) = xai_grok_shell::config::load_effective_config() else {
        return false;
    };
    let Ok(cfg) = xai_grok_shell::agent::config::Config::new_from_toml_cfg(&raw) else {
        return false;
    };
    cfg.grok_com_config
        .auth_provider_command
        .as_deref()
        .is_some_and(|s| !s.trim().is_empty())
}

/// Parse `/usage bar [on|off]` → target visibility, or `None` if not a bar arg.
fn parse_bar_arg(arg: &str) -> Option<Result<bool, String>> {
    let mut parts = arg.split_whitespace();
    let head = parts.next()?;
    if !head.eq_ignore_ascii_case("bar") {
        return None;
    }
    match parts.next() {
        None => Some(Ok(!crate::appearance::cache::load_show_limit_bar())),
        Some(v) if v.eq_ignore_ascii_case("on") || v == "1" || v.eq_ignore_ascii_case("true") => {
            Some(Ok(true))
        }
        Some(v) if v.eq_ignore_ascii_case("off") || v == "0" || v.eq_ignore_ascii_case("false") => {
            Some(Ok(false))
        }
        Some(other) => Some(Err(format!(
            "Unknown argument: bar {other}. Use /usage bar [on|off]"
        ))),
    }
}

impl SlashCommand for UsageCommand {
    fn name(&self) -> &str {
        "usage"
    }

    fn aliases(&self) -> &[&str] {
        &["cost"]
    }

    fn description(&self) -> &str {
        "View usage; toggle limit bar"
    }

    fn usage(&self) -> &str {
        "/usage [show|manage|bar [on|off]]"
    }

    fn takes_args(&self) -> bool {
        true
    }

    fn visible(&self, ctx: &AppCtx) -> bool {
        ctx.usage_command_visible
    }

    fn takes_args_now(&self, ctx: &AppCtx) -> bool {
        // Always offer args when the command is visible so `/usage bar` works
        // even when the full billing surface is hidden.
        ctx.usage_command_visible
    }

    fn suggest_args(&self, ctx: &AppCtx, args_query: &str) -> Option<Vec<ArgItem>> {
        if !ctx.usage_command_visible {
            return None;
        }
        let q = args_query.trim_start();
        // Nested suggestions after typing `bar `.
        if q.to_ascii_lowercase().starts_with("bar") {
            let after = q.get(3..).unwrap_or("").trim_start();
            if q.len() >= 3 && (q.len() == 3 || q.as_bytes().get(3) == Some(&b' ')) {
                return Some(vec![
                    ArgItem {
                        display: "on".into(),
                        match_text: "on".into(),
                        insert_text: "on".into(),
                        description: "Show the usage limit bar".into(),
                    },
                    ArgItem {
                        display: "off".into(),
                        match_text: "off".into(),
                        insert_text: "off".into(),
                        description: "Hide the usage limit bar".into(),
                    },
                ]
                .into_iter()
                .filter(|item| after.is_empty() || item.match_text.starts_with(after))
                .collect());
            }
        }
        let mut items = vec![
            ArgItem {
                display: "show".into(),
                match_text: "show".into(),
                insert_text: "show".into(),
                description: "View usage".into(),
            },
            ArgItem {
                display: "bar".into(),
                match_text: "bar".into(),
                insert_text: "bar".into(),
                description: "Toggle usage limit bar on prompt".into(),
            },
        ];
        if ctx.billing_surface_visible {
            items.push(ArgItem {
                display: "manage".into(),
                match_text: "manage".into(),
                insert_text: "manage".into(),
                description: "Manage billing".into(),
            });
        }
        Some(items)
    }

    fn run(&self, ctx: &mut CommandExecCtx, args: &str) -> CommandResult {
        if !ctx.usage_command_visible {
            return CommandResult::Error("/usage is not available.".into());
        }
        let arg = args.trim();
        if let Some(bar) = parse_bar_arg(arg) {
            return match bar {
                Ok(new) => CommandResult::Action(Action::SetLimitBar(new)),
                Err(msg) => CommandResult::Error(msg),
            };
        }
        if !ctx.billing_surface_visible {
            return match arg {
                "" | "show" => CommandResult::Action(Action::ShowUsage),
                _ => CommandResult::Error(format!(
                    "Unknown argument: {arg}. Use /usage [show|bar [on|off]]"
                )),
            };
        }
        match arg {
            "" | "show" => CommandResult::Action(Action::ShowUsage),
            "manage" => CommandResult::Action(Action::ManageBilling),
            _ => CommandResult::Error(format!(
                "Unknown argument: {arg}. Use /usage show, /usage manage, or /usage bar [on|off]"
            )),
        }
    }
}
