//! Provider model discovery and its small persistent cache.
//!
//! Codex exposes a machine-readable model catalog. Claude Code currently
//! exposes its effort choices in `--help`, but not a model catalog, so its
//! versioned model IDs are kept here until the CLI can enumerate them.

use std::{
    io::Read,
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        OnceLock,
    },
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::{
    app::Runtime,
    dependencies::resolve_executable,
    domain::{AgentKind, GrillAgentCatalog, GrillEffort, GrillModel},
    features::SharedRuntime,
};

const CODEX_CATALOG_KEY: &str = "grill_codex_model_catalog";
const CODEX_CATALOG_ERROR_KEY: &str = "grill_codex_model_catalog_error";
const CATALOG_TTL_SECS: i64 = 24 * 60 * 60;
const RETRY_DELAY_SECS: i64 = 5 * 60;
const COMMAND_TIMEOUT: Duration = Duration::from_secs(20);
static CODEX_REFRESH_IN_PROGRESS: AtomicBool = AtomicBool::new(false);
static CODEX_LAST_ATTEMPT_AT: std::sync::atomic::AtomicI64 = std::sync::atomic::AtomicI64::new(0);
static CLAUDE_EFFORTS: OnceLock<Vec<GrillEffort>> = OnceLock::new();

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GrillModelCatalogSnapshot {
    pub catalogs: Vec<GrillAgentCatalog>,
    pub codex_status: CatalogRefreshStatus,
    pub codex_error: Option<String>,
    pub codex_fetched_at: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum CatalogRefreshStatus {
    Ready,
    Refreshing,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CodexCatalogCache {
    fetched_at: i64,
    models: Vec<GrillModel>,
}

pub(crate) fn list(app: AppHandle, runtime: &Runtime) -> Result<GrillModelCatalogSnapshot, String> {
    let cache = runtime
        .store
        .setting(CODEX_CATALOG_KEY)
        .map_err(|error| error.to_string())?
        .and_then(|serialized| serde_json::from_str::<CodexCatalogCache>(&serialized).ok());
    let refresh_error = runtime
        .store
        .setting(CODEX_CATALOG_ERROR_KEY)
        .map_err(|error| error.to_string())?
        .filter(|error| !error.trim().is_empty());

    let is_stale = cache
        .as_ref()
        .is_none_or(|cache| unix_seconds().saturating_sub(cache.fetched_at) >= CATALOG_TTL_SECS);
    if is_stale {
        start_refresh(app, false);
    }

    let refreshing = CODEX_REFRESH_IN_PROGRESS.load(Ordering::Acquire);
    let (codex_status, codex_error) = if refreshing {
        (CatalogRefreshStatus::Refreshing, None)
    } else if is_stale {
        (
            CatalogRefreshStatus::Error,
            Some(refresh_error.unwrap_or_else(|| "Codex model discovery is unavailable".into())),
        )
    } else {
        (CatalogRefreshStatus::Ready, None)
    };

    let mut catalogs = Vec::new();
    let claude = claude_catalog();
    if !claude.models.is_empty() {
        catalogs.push(claude);
    }
    if let Some(cache) = cache {
        catalogs.push(GrillAgentCatalog {
            agent: AgentKind::Codex,
            models: cache.models,
        });
    }

    Ok(GrillModelCatalogSnapshot {
        catalogs,
        codex_status,
        codex_error,
        codex_fetched_at: cache_fetched_at(runtime)?,
    })
}

pub(crate) fn refresh(app: AppHandle) {
    start_refresh(app, true);
}

fn cache_fetched_at(runtime: &Runtime) -> Result<Option<i64>, String> {
    let Some(serialized) = runtime
        .store
        .setting(CODEX_CATALOG_KEY)
        .map_err(|error| error.to_string())?
    else {
        return Ok(None);
    };
    Ok(serde_json::from_str::<CodexCatalogCache>(&serialized)
        .ok()
        .map(|cache| cache.fetched_at))
}

fn start_refresh(app: AppHandle, force: bool) {
    let now = unix_seconds();
    let last_attempt = CODEX_LAST_ATTEMPT_AT.load(Ordering::Acquire);
    if !force && now.saturating_sub(last_attempt) < RETRY_DELAY_SECS {
        return;
    }
    if CODEX_REFRESH_IN_PROGRESS
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return;
    }
    CODEX_LAST_ATTEMPT_AT.store(now, Ordering::Release);

    thread::spawn(move || {
        let result = discover_codex_catalog();
        let state = app.state::<SharedRuntime>();
        if let Ok(mut runtime) = state.lock() {
            match result {
                Ok(models) => {
                    let cache = CodexCatalogCache {
                        fetched_at: unix_seconds(),
                        models,
                    };
                    match serde_json::to_string(&cache) {
                        Ok(serialized) => {
                            let _ = runtime.store.set_setting(CODEX_CATALOG_KEY, &serialized);
                            let _ = runtime.store.set_setting(CODEX_CATALOG_ERROR_KEY, "");
                        }
                        Err(_) => {
                            let _ = runtime.store.set_setting(
                                CODEX_CATALOG_ERROR_KEY,
                                "Could not save the Codex model catalog",
                            );
                        }
                    }
                }
                Err(message) => {
                    let _ = runtime.store.set_setting(CODEX_CATALOG_ERROR_KEY, &message);
                }
            }
        }
        CODEX_REFRESH_IN_PROGRESS.store(false, Ordering::Release);
    });
}

fn discover_codex_catalog() -> Result<Vec<GrillModel>, String> {
    let executable = resolve_executable("codex", None)
        .ok_or_else(|| "Codex CLI was not found on PATH".to_owned())?;
    let output = run_command(&executable, &["debug", "models"])?;
    let payload: serde_json::Value = serde_json::from_slice(&output)
        .map_err(|_| "Codex CLI returned an invalid model catalog".to_owned())?;
    let models = payload
        .get("models")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "Codex CLI returned no model catalog".to_owned())?;

    let mut result = Vec::new();
    for model in models {
        if model
            .get("visibility")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|visibility| visibility.eq_ignore_ascii_case("hidden"))
        {
            continue;
        }
        let Some(id) = model.get("slug").and_then(serde_json::Value::as_str) else {
            continue;
        };
        let Some(levels) = model
            .get("supported_reasoning_levels")
            .and_then(serde_json::Value::as_array)
        else {
            continue;
        };
        let default_level = model
            .get("default_reasoning_level")
            .and_then(serde_json::Value::as_str);
        let mut efforts = levels
            .iter()
            .filter_map(|level| {
                let id = level.get("effort").and_then(serde_json::Value::as_str)?;
                Some(GrillEffort {
                    id: id.to_owned(),
                    label: effort_label(id),
                })
            })
            .collect::<Vec<_>>();
        if let Some(default_level) = default_level {
            if let Some(index) = efforts.iter().position(|effort| effort.id == default_level) {
                efforts.swap(0, index);
            }
        }
        if efforts.is_empty() {
            continue;
        }

        let label = model
            .get("display_name")
            .and_then(serde_json::Value::as_str)
            .filter(|label| !label.trim().is_empty())
            .unwrap_or(id)
            .to_owned();
        result.push(GrillModel {
            id: id.to_owned(),
            label,
            efforts,
        });
    }

    if result.is_empty() {
        return Err("Codex CLI returned no usable models".to_owned());
    }
    Ok(result)
}

fn claude_catalog() -> GrillAgentCatalog {
    let efforts = discover_claude_efforts();
    let models = crate::domain::grill_model_catalog()
        .into_iter()
        .find(|catalog| catalog.agent == AgentKind::Claude)
        .map(|catalog| {
            if efforts.is_empty() {
                return Vec::new();
            }
            catalog
                .models
                .into_iter()
                .map(|mut model| {
                    model.efforts = efforts.clone();
                    model
                })
                .collect()
        })
        .unwrap_or_default();

    GrillAgentCatalog {
        agent: AgentKind::Claude,
        models,
    }
}

fn discover_claude_efforts() -> Vec<GrillEffort> {
    CLAUDE_EFFORTS.get_or_init(read_claude_efforts).clone()
}

fn read_claude_efforts() -> Vec<GrillEffort> {
    let Some(executable) = resolve_executable("claude", None) else {
        return Vec::new();
    };
    let Ok(output) = run_command(&executable, &["--help"]) else {
        return Vec::new();
    };
    let help = String::from_utf8_lossy(&output).replace('\n', " ");
    let Some(effort_option) = help.find("--effort <level>") else {
        return Vec::new();
    };
    let effort_help = &help[effort_option..];
    let Some(open) = effort_help.find('(') else {
        return Vec::new();
    };
    let Some(close) = effort_help[open + 1..].find(')') else {
        return Vec::new();
    };
    effort_help[open + 1..open + 1 + close]
        .split(',')
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .map(|id| GrillEffort {
            id: id.to_owned(),
            label: effort_label(id),
        })
        .collect()
}

fn effort_label(id: &str) -> String {
    match id {
        "xhigh" => "Extra high".into(),
        "ultra" => "Ultra".into(),
        _ => {
            let mut characters = id.chars();
            characters
                .next()
                .map(|first| first.to_uppercase().collect::<String>() + characters.as_str())
                .unwrap_or_default()
        }
    }
}

fn run_command(executable: &std::path::Path, arguments: &[&str]) -> Result<Vec<u8>, String> {
    let mut child = Command::new(executable)
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| "Could not start the provider CLI".to_owned())?;
    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| "Could not read the provider CLI output".to_owned())?;
    let reader = thread::spawn(move || {
        let mut output = Vec::new();
        stdout.read_to_end(&mut output).map(|_| output)
    });
    let started = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let output = reader
                    .join()
                    .map_err(|_| "Could not read the provider CLI output".to_owned())?
                    .map_err(|_| "Could not read the provider CLI output".to_owned())?;
                if !status.success() {
                    return Err("Provider CLI could not discover its model catalog".to_owned());
                }
                return Ok(output);
            }
            Ok(None) if started.elapsed() < COMMAND_TIMEOUT => {
                thread::sleep(Duration::from_millis(100));
            }
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = reader.join();
                return Err("Provider model discovery timed out".to_owned());
            }
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = reader.join();
                return Err("Could not read provider CLI status".to_owned());
            }
        }
    }
}

fn unix_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs().min(i64::MAX as u64) as i64)
        .unwrap_or_default()
}
