//! Subscription plan usage for every Agent CLI Configuration Profile.
//!
//! Neither provider CLI exposes a `usage` subcommand, so each one is read
//! through the cheapest route that needs no credential of our own:
//!
//! * Codex answers `account/rateLimits/read` over its app-server JSON-RPC
//!   pipe, which is a live backend read independent of any agent turn.
//! * Claude Code has no equivalent, so its own cache of the usage response is
//!   read from `.claude.json` inside the profile directory. That file also
//!   holds account state we have no business keeping, so only the usage
//!   numbers survive parsing; nothing else is retained or persisted.
//!
//! Both routes run through `run_machine_shell`, so a profile on a remote
//! Machine is read exactly like a local one.

use std::{
    sync::{
        atomic::{AtomicBool, AtomicI64, Ordering},
        Arc,
    },
    thread,
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::{
    app::Runtime,
    domain::{AgentKind, CliConfigurationProfile, Machine},
    features::SharedRuntime,
    terminal::{find_agent_executable, run_machine_shell, shell_quote},
};

const SNAPSHOT_KEY: &str = "plan_usage_snapshot";
/// Codex is a live read and Claude is a cache someone else refreshes, so a
/// few minutes is as fresh as this can honestly claim to be.
const SNAPSHOT_TTL_SECS: i64 = 5 * 60;
const RETRY_DELAY_SECS: i64 = 60;
/// `.claude.json` grows with project history; anything past this is not a
/// usage cache and is refused rather than pulled across an SSH connection.
const CLAUDE_STATE_BYTE_LIMIT: usize = 32 * 1024 * 1024;
/// How long the Codex app-server pipe stays open waiting for its reply.
const CODEX_REPLY_WAIT_SECS: u32 = 8;

static REFRESH_IN_PROGRESS: AtomicBool = AtomicBool::new(false);
static LAST_ATTEMPT_AT: AtomicI64 = AtomicI64::new(0);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PlanUsageSnapshot {
    pub profiles: Vec<ProfilePlanUsage>,
    pub fetched_at: Option<i64>,
    pub status: PlanUsageRefreshStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum PlanUsageRefreshStatus {
    Ready,
    Refreshing,
    Never,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProfilePlanUsage {
    pub profile_id: i64,
    pub profile_name: String,
    pub provider: AgentKind,
    pub machine_id: i64,
    pub machine_name: String,
    pub state: ProfileUsageState,
    /// Why the profile has no windows, in the user's terms.
    pub detail: Option<String>,
    pub plan: Option<String>,
    /// When the provider itself produced these numbers, when it says so.
    pub observed_at: Option<i64>,
    pub windows: Vec<UsageWindow>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum ProfileUsageState {
    Ready,
    SignedOut,
    MachineUnreachable,
    NotReported,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UsageWindow {
    pub id: String,
    pub label: String,
    /// Always 0-100, whatever scale the provider reported.
    pub used_percent: f64,
    pub resets_at: Option<i64>,
}

pub(crate) fn list(app: AppHandle, runtime: &Runtime) -> Result<PlanUsageSnapshot, String> {
    let cached = stored_snapshot(runtime)?;
    let is_stale = cached.as_ref().is_none_or(|snapshot| {
        snapshot
            .fetched_at
            .is_none_or(|fetched_at| unix_seconds() - fetched_at >= SNAPSHOT_TTL_SECS)
    });
    if is_stale {
        start_refresh(app, false);
    }

    let refreshing = REFRESH_IN_PROGRESS.load(Ordering::Acquire);
    Ok(match cached {
        Some(mut snapshot) => {
            if refreshing {
                snapshot.status = PlanUsageRefreshStatus::Refreshing;
            }
            snapshot
        }
        None => PlanUsageSnapshot {
            profiles: Vec::new(),
            fetched_at: None,
            status: if refreshing {
                PlanUsageRefreshStatus::Refreshing
            } else {
                PlanUsageRefreshStatus::Never
            },
        },
    })
}

pub(crate) fn refresh(app: AppHandle) {
    start_refresh(app, true);
}

fn stored_snapshot(runtime: &Runtime) -> Result<Option<PlanUsageSnapshot>, String> {
    Ok(runtime
        .store
        .setting(SNAPSHOT_KEY)
        .map_err(|error| error.to_string())?
        .and_then(|serialized| serde_json::from_str::<PlanUsageSnapshot>(&serialized).ok()))
}

fn start_refresh(app: AppHandle, force: bool) {
    let now = unix_seconds();
    if !force && now - LAST_ATTEMPT_AT.load(Ordering::Acquire) < RETRY_DELAY_SECS {
        return;
    }
    if REFRESH_IN_PROGRESS
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return;
    }
    LAST_ATTEMPT_AT.store(now, Ordering::Release);

    thread::spawn(move || {
        let targets = read_targets(&app);
        let profiles = targets
            .iter()
            .map(|(machine, profile)| read_profile(machine, profile))
            .collect::<Vec<_>>();
        let snapshot = PlanUsageSnapshot {
            profiles,
            fetched_at: Some(unix_seconds()),
            status: PlanUsageRefreshStatus::Ready,
        };
        if let Ok(serialized) = serde_json::to_string(&snapshot) {
            let state = app.state::<SharedRuntime>();
            let stored = state.lock();
            if let Ok(mut runtime) = stored {
                let _ = runtime.store.set_setting(SNAPSHOT_KEY, &serialized);
            }
        }
        REFRESH_IN_PROGRESS.store(false, Ordering::Release);
    });
}

/// The profile list is copied out under the lock so the reads themselves,
/// which spawn processes and open SSH connections, never hold the Runtime.
fn read_targets(app: &AppHandle) -> Vec<(Arc<Machine>, CliConfigurationProfile)> {
    let state = app.state::<SharedRuntime>();
    let Ok(runtime) = state.lock() else {
        return Vec::new();
    };
    let machines = runtime
        .state
        .machines
        .iter()
        .cloned()
        .map(Arc::new)
        .collect::<Vec<_>>();
    runtime
        .state
        .cli_configuration_profiles
        .iter()
        .filter_map(|profile| {
            let machine = machines
                .iter()
                .find(|machine| machine.id == profile.machine_id)?;
            Some((Arc::clone(machine), profile.clone()))
        })
        .collect()
}

fn read_profile(machine: &Machine, profile: &CliConfigurationProfile) -> ProfilePlanUsage {
    let reading = match profile.provider {
        AgentKind::Claude => read_claude(machine, profile),
        AgentKind::Codex => read_codex(machine, profile),
    };
    let base = ProfilePlanUsage {
        profile_id: profile.id,
        profile_name: profile.name.clone(),
        provider: profile.provider,
        machine_id: machine.id,
        machine_name: machine.name.clone(),
        state: ProfileUsageState::NotReported,
        detail: None,
        plan: None,
        observed_at: None,
        windows: Vec::new(),
    };
    match reading {
        Ok(reading) => ProfilePlanUsage {
            state: ProfileUsageState::Ready,
            plan: reading.plan,
            observed_at: reading.observed_at,
            windows: reading.windows,
            ..base
        },
        Err(failure) => ProfilePlanUsage {
            state: failure.state,
            detail: Some(failure.detail),
            ..base
        },
    }
}

#[derive(Debug)]
struct UsageReading {
    plan: Option<String>,
    observed_at: Option<i64>,
    windows: Vec<UsageWindow>,
}

#[derive(Debug)]
struct UsageFailure {
    state: ProfileUsageState,
    detail: String,
}

impl UsageFailure {
    fn not_reported(detail: impl Into<String>) -> Self {
        Self {
            state: ProfileUsageState::NotReported,
            detail: detail.into(),
        }
    }
}

/// Distinguishes "the Machine did not answer" from "the CLI answered badly",
/// because only the first one means every profile on that Machine is blind.
fn run_profile_shell(machine: &Machine, command: &str) -> Result<String, UsageFailure> {
    run_machine_shell(machine, command).map_err(|error| UsageFailure {
        state: ProfileUsageState::MachineUnreachable,
        detail: error,
    })
}

fn read_claude(
    machine: &Machine,
    profile: &CliConfigurationProfile,
) -> Result<UsageReading, UsageFailure> {
    let directory = shell_quote(&profile.directory);
    let command = format!(
        "set -eu; directory={directory}; \
         for candidate in \"$directory/.claude.json\" \"$HOME/.claude.json\"; do \
           if [ -f \"$candidate\" ]; then head -c {CLAUDE_STATE_BYTE_LIMIT} -- \"$candidate\"; exit 0; fi; \
         done; exit 0"
    );
    parse_claude_usage(&run_profile_shell(machine, &command)?)
}

fn parse_claude_usage(payload: &str) -> Result<UsageReading, UsageFailure> {
    if payload.trim().is_empty() {
        return Err(UsageFailure {
            state: ProfileUsageState::SignedOut,
            detail: "Claude Code has not been run with this profile yet".into(),
        });
    }
    let state: serde_json::Value = serde_json::from_str(payload)
        .map_err(|_| UsageFailure::not_reported("Claude Code state could not be read"))?;
    let cached = state.get("cachedUsageUtilization").ok_or_else(|| {
        UsageFailure::not_reported("Claude Code has not recorded plan usage for this profile yet")
    })?;
    let utilization = cached
        .get("utilization")
        .ok_or_else(|| UsageFailure::not_reported("Claude Code reported no plan usage"))?;

    let mut windows = Vec::new();
    for (id, label) in CLAUDE_WINDOWS {
        let Some(window) = utilization.get(id).filter(|window| !window.is_null()) else {
            continue;
        };
        let Some(fraction) = window
            .get("utilization")
            .and_then(serde_json::Value::as_f64)
        else {
            continue;
        };
        windows.push(UsageWindow {
            id: (*id).to_owned(),
            label: (*label).to_owned(),
            // These windows are reported as a 0-1 fraction.
            used_percent: (fraction * 100.0).clamp(0.0, 100.0),
            resets_at: window.get("resets_at").and_then(serde_json::Value::as_i64),
        });
    }

    // Extra usage is already a percentage and is the only window some plans
    // meter against, so a row without it would look empty for those accounts.
    if let Some(extra) = utilization
        .get("extra_usage")
        .filter(|extra| extra.get("is_enabled").and_then(serde_json::Value::as_bool) == Some(true))
    {
        if let Some(percent) = extra.get("utilization").and_then(serde_json::Value::as_f64) {
            windows.push(UsageWindow {
                id: "extra_usage".into(),
                label: "Extra usage".into(),
                used_percent: percent.clamp(0.0, 100.0),
                resets_at: None,
            });
        }
    }

    if windows.is_empty() {
        return Err(UsageFailure::not_reported(
            "Claude Code reported no plan limits for this profile",
        ));
    }
    Ok(UsageReading {
        plan: None,
        observed_at: cached
            .get("fetchedAtMs")
            .and_then(serde_json::Value::as_i64)
            .map(|milliseconds| milliseconds / 1_000),
        windows,
    })
}

const CLAUDE_WINDOWS: &[(&str, &str)] = &[
    ("five_hour", "5-hour window"),
    ("seven_day", "Weekly"),
    ("seven_day_opus", "Weekly (Opus)"),
    ("seven_day_sonnet", "Weekly (Sonnet)"),
];

fn read_codex(
    machine: &Machine,
    profile: &CliConfigurationProfile,
) -> Result<UsageReading, UsageFailure> {
    let executable = find_agent_executable(machine, "codex").map_err(UsageFailure::not_reported)?;
    let directory = shell_quote(&profile.directory);
    let executable = shell_quote(&executable.to_string_lossy());
    let command = format!(
        "set -eu; export CODEX_HOME={directory}; \
         {{ printf '%s\\n' '{INITIALIZE}'; printf '%s\\n' '{INITIALIZED}'; \
            printf '%s\\n' '{READ_RATE_LIMITS}'; sleep {CODEX_REPLY_WAIT_SECS}; }} \
         | {executable} app-server 2>/dev/null"
    );
    parse_codex_usage(&run_profile_shell(machine, &command)?)
}

fn parse_codex_usage(payload: &str) -> Result<UsageReading, UsageFailure> {
    let reply = payload
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .find(|message| message.get("id").and_then(serde_json::Value::as_i64) == Some(2))
        .ok_or_else(|| UsageFailure {
            state: ProfileUsageState::SignedOut,
            detail: "Codex did not report plan usage; the profile may not be signed in".into(),
        })?;
    if let Some(error) = reply.get("error") {
        return Err(UsageFailure {
            state: ProfileUsageState::SignedOut,
            detail: error
                .get("message")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("Codex refused to report plan usage")
                .to_owned(),
        });
    }
    let limits = reply
        .get("result")
        .and_then(|result| result.get("rateLimits"))
        .ok_or_else(|| UsageFailure::not_reported("Codex reported no plan limits"))?;

    let windows = ["primary", "secondary"]
        .into_iter()
        .filter_map(|key| codex_window(key, limits.get(key)?))
        .collect::<Vec<_>>();
    if windows.is_empty() {
        return Err(UsageFailure::not_reported("Codex reported no plan limits"));
    }
    Ok(UsageReading {
        plan: limits
            .get("planType")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned),
        // The app-server read is live, so the snapshot time is the only
        // timestamp there is.
        observed_at: None,
        windows,
    })
}

const INITIALIZE: &str = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"clientInfo":{"name":"ai-mission-manager","version":"1"}}}"#;
const INITIALIZED: &str = r#"{"jsonrpc":"2.0","method":"initialized","params":{}}"#;
const READ_RATE_LIMITS: &str =
    r#"{"jsonrpc":"2.0","id":2,"method":"account/rateLimits/read","params":{}}"#;

fn codex_window(key: &str, window: &serde_json::Value) -> Option<UsageWindow> {
    let used_percent = window
        .get("usedPercent")
        .and_then(serde_json::Value::as_f64)?;
    let minutes = window
        .get("windowDurationMins")
        .and_then(serde_json::Value::as_i64);
    Some(UsageWindow {
        id: key.to_owned(),
        label: window_label(minutes),
        used_percent: used_percent.clamp(0.0, 100.0),
        resets_at: window.get("resetsAt").and_then(serde_json::Value::as_i64),
    })
}

/// Codex names its windows by duration rather than by meaning, so the label
/// is derived from the duration it reports instead of being assumed.
fn window_label(minutes: Option<i64>) -> String {
    match minutes {
        Some(300) => "5-hour window".into(),
        Some(10080) => "Weekly".into(),
        Some(minutes) if minutes % 1440 == 0 => format!("{}-day window", minutes / 1440),
        Some(minutes) if minutes % 60 == 0 => format!("{}-hour window", minutes / 60),
        Some(minutes) => format!("{minutes}-minute window"),
        None => "Plan limit".into(),
    }
}

fn unix_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs().min(i64::MAX as u64) as i64)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_windows_are_read_as_percentages() {
        let reading = parse_claude_usage(
            r#"{"cachedUsageUtilization":{"fetchedAtMs":1790709887000,"utilization":{
                "five_hour":{"utilization":0.42,"resets_at":1790731127},
                "seven_day":{"utilization":0.1,"resets_at":1791203784},
                "seven_day_opus":null,
                "seven_day_sonnet":{"utilization":0.9,"resets_at":1791203784}}}}"#,
        )
        .expect("a cached utilization should be readable");

        assert_eq!(reading.observed_at, Some(1790709887));
        assert_eq!(
            reading
                .windows
                .iter()
                .map(|window| (window.id.as_str(), window.used_percent))
                .collect::<Vec<_>>(),
            vec![
                ("five_hour", 42.0),
                ("seven_day", 10.0),
                ("seven_day_sonnet", 90.0)
            ]
        );
    }

    #[test]
    fn claude_extra_usage_is_reported_when_windows_are_empty() {
        let reading = parse_claude_usage(
            r#"{"cachedUsageUtilization":{"utilization":{
                "five_hour":null,"seven_day":null,
                "extra_usage":{"is_enabled":true,"utilization":100}}}}"#,
        )
        .expect("extra usage alone should still be a reading");

        assert_eq!(reading.windows.len(), 1);
        assert_eq!(reading.windows[0].id, "extra_usage");
        assert_eq!(reading.windows[0].used_percent, 100.0);
        assert_eq!(reading.windows[0].resets_at, None);
    }

    #[test]
    fn claude_without_a_cache_is_not_reported_rather_than_zero() {
        let failure = parse_claude_usage(r#"{"projects":{}}"#)
            .expect_err("a state file with no usage cache is not a reading");
        assert_eq!(failure.state, ProfileUsageState::NotReported);
    }

    #[test]
    fn a_missing_claude_state_file_reads_as_signed_out() {
        let failure =
            parse_claude_usage("  \n").expect_err("no state file at all is not a reading");
        assert_eq!(failure.state, ProfileUsageState::SignedOut);
    }

    #[test]
    fn codex_windows_are_labelled_by_the_duration_codex_reports() {
        let reading = parse_codex_usage(
            r#"{"jsonrpc":"2.0","method":"loginChatGptComplete","params":{}}
{"id":2,"result":{"rateLimits":{"planType":"plus","primary":{"usedPercent":2,"windowDurationMins":300,"resetsAt":1790731127},"secondary":{"usedPercent":8,"windowDurationMins":10080,"resetsAt":1791203785}}}}"#,
        )
        .expect("a rate limit reply should be readable");

        assert_eq!(reading.plan.as_deref(), Some("plus"));
        assert_eq!(
            reading
                .windows
                .iter()
                .map(|window| (window.label.as_str(), window.used_percent, window.resets_at))
                .collect::<Vec<_>>(),
            vec![
                ("5-hour window", 2.0, Some(1790731127)),
                ("Weekly", 8.0, Some(1791203785)),
            ]
        );
    }

    #[test]
    fn a_codex_error_reply_reads_as_signed_out() {
        let failure =
            parse_codex_usage(r#"{"id":2,"error":{"code":-32000,"message":"Not logged in"}}"#)
                .expect_err("an error reply is not a reading");

        assert_eq!(failure.state, ProfileUsageState::SignedOut);
        assert_eq!(failure.detail, "Not logged in");
    }

    #[test]
    fn a_silent_codex_app_server_reads_as_signed_out() {
        let failure = parse_codex_usage("").expect_err("no reply is not a reading");
        assert_eq!(failure.state, ProfileUsageState::SignedOut);
    }
}
