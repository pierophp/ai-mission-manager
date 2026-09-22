use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const GRILL_SKILL_SNAPSHOT: &str = include_str!("../../.agents/skills/grilling/SKILL.md");
pub const TO_SPEC_SKILL_SNAPSHOT: &str = include_str!("../../.agents/skills/to-spec/SKILL.md");
pub const TO_TICKETS_SKILL_SNAPSHOT: &str =
    include_str!("../../.agents/skills/to-tickets/SKILL.md");
pub const IMPLEMENT_SKILL_SNAPSHOT: &str = include_str!("../../.agents/skills/implement/SKILL.md");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GrillContinuationAction {
    ToSpec,
    ToTickets,
    Implement,
}

impl GrillContinuationAction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ToSpec => "to-spec",
            Self::ToTickets => "to-tickets",
            Self::Implement => "implement",
        }
    }

    pub fn skill_snapshot(self) -> &'static str {
        match self {
            Self::ToSpec => TO_SPEC_SKILL_SNAPSHOT,
            Self::ToTickets => TO_TICKETS_SKILL_SNAPSHOT,
            Self::Implement => IMPLEMENT_SKILL_SNAPSHOT,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DownstreamIssueDiscovery {
    StructuredEvent,
    OutputUrl,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownstreamIssueCandidate {
    pub url: String,
    pub discovery: DownstreamIssueDiscovery,
    #[serde(default)]
    pub run_id: Option<i64>,
    #[serde(default)]
    pub action: Option<GrillContinuationAction>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GithubIssueCreatedEvent {
    #[serde(alias = "type", alias = "kind")]
    pub event: String,
    pub url: String,
    #[serde(default)]
    pub run_id: Option<i64>,
    #[serde(default)]
    pub action: Option<GrillContinuationAction>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfirmedDownstreamIssue {
    pub object: ExternalObjectInput,
    pub snapshot: ExternalSnapshotData,
    pub discovery: DownstreamIssueDiscovery,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinkProvenance {
    pub run_id: i64,
    pub action: GrillContinuationAction,
    pub discovery: DownstreamIssueDiscovery,
}

const DOWNSTREAM_EVENT_PREFIX: &str = "AI_MISSION_MANAGER_EVENT ";

pub fn discover_downstream_issue_candidates(output: &str) -> Vec<DownstreamIssueCandidate> {
    let structured = output
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            let json = trimmed
                .strip_prefix(DOWNSTREAM_EVENT_PREFIX)
                .or_else(|| trimmed.starts_with('{').then_some(trimmed))?;
            let event = serde_json::from_str::<GithubIssueCreatedEvent>(json).ok()?;
            (event.event == "github.issue.created").then_some(DownstreamIssueCandidate {
                url: event.url,
                discovery: DownstreamIssueDiscovery::StructuredEvent,
                run_id: event.run_id,
                action: event.action,
            })
        })
        .collect::<Vec<_>>();
    if !structured.is_empty() {
        return unique_issue_candidates(structured);
    }

    unique_issue_candidates(
        output
            .split_whitespace()
            .filter_map(|token| {
                let url = token
                    .trim_matches(|character: char| {
                        matches!(
                            character,
                            '(' | ')'
                                | '['
                                | ']'
                                | '{'
                                | '}'
                                | '<'
                                | '>'
                                | '"'
                                | '\''
                                | '`'
                                | '.'
                                | ','
                                | ';'
                                | ':'
                                | '!'
                                | '?'
                        )
                    })
                    .trim_end_matches('/');
                is_github_issue_url(url).then_some(DownstreamIssueCandidate {
                    url: url.to_owned(),
                    discovery: DownstreamIssueDiscovery::OutputUrl,
                    run_id: None,
                    action: None,
                })
            })
            .collect(),
    )
}

fn unique_issue_candidates(
    candidates: Vec<DownstreamIssueCandidate>,
) -> Vec<DownstreamIssueCandidate> {
    candidates
        .into_iter()
        .fold(Vec::new(), |mut unique, candidate| {
            if !unique.iter().any(|existing: &DownstreamIssueCandidate| {
                existing.url.eq_ignore_ascii_case(&candidate.url)
            }) {
                unique.push(candidate);
            }
            unique
        })
}

fn is_github_issue_url(url: &str) -> bool {
    let parts = url.split('/').collect::<Vec<_>>();
    parts.len() == 7
        && parts[0] == "https:"
        && (parts[2].eq_ignore_ascii_case("github.com")
            || parts[2].eq_ignore_ascii_case("www.github.com"))
        && !parts[3].is_empty()
        && !parts[4].is_empty()
        && parts[5] == "issues"
        && parts[6].parse::<u64>().is_ok()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GrillConfiguration {
    pub agent: AgentKind,
    pub model: String,
    pub effort: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GrillQuestionGroup {
    pub questions: Vec<GrillQuestion>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GrillQuestion {
    pub number: u32,
    pub title: Option<String>,
    pub prompt: String,
    pub recommendation: Option<String>,
    pub options: Vec<GrillOption>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GrillOption {
    pub key: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GrillAnswer {
    pub question_number: u32,
    pub answer: String,
}

/// Parse the stable markers emitted by the grilling skill without coupling the
/// application to a terminal, agent, or filesystem.
pub fn parse_grill_question_group(transcript: &str) -> Option<GrillQuestionGroup> {
    let transcript = strip_terminal_escape_sequences(transcript);
    let mut questions = Vec::new();
    let mut current: Option<GrillQuestion> = None;

    for line in transcript.lines() {
        let line = line.trim_end();
        if let Some(header) = line.trim_start().strip_prefix("❓") {
            if let Some(question) = current.take() {
                if !question.prompt.is_empty() {
                    questions.push(question);
                }
            }
            let (number, title, prompt) = parse_question_header(header, questions.len() as u32 + 1);
            current = Some(GrillQuestion {
                number,
                title,
                prompt,
                recommendation: None,
                options: Vec::new(),
            });
            continue;
        }

        let Some(question) = current.as_mut() else {
            continue;
        };
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed == "---" {
            continue;
        }
        if let Some(recommendation) = trimmed
            .strip_prefix("➡️")
            .or_else(|| trimmed.strip_prefix("➡"))
        {
            let recommendation = clean_markup(recommendation);
            if !recommendation.is_empty() {
                question.recommendation = Some(recommendation);
            }
            continue;
        }
        if let Some((key, label)) = parse_grill_option(trimmed) {
            question.options.push(GrillOption { key, label });
            continue;
        }

        if !question.prompt.is_empty() {
            question.prompt.push('\n');
        }
        question.prompt.push_str(trimmed);
    }

    if let Some(question) = current {
        if !question.prompt.is_empty() {
            questions.push(question);
        }
    }
    (!questions.is_empty()).then_some(GrillQuestionGroup { questions })
}

/// Parse only the newly captured portion of a Pane transcript. The Terminal
/// Runtime retains scrollback, so reparsing the whole Pane would surface an
/// earlier question group when a downstream skill asks a new one.
pub fn parse_grill_question_group_since(
    previous_transcript: &str,
    transcript: &str,
) -> Option<GrillQuestionGroup> {
    let previous_transcript = strip_terminal_escape_sequences(previous_transcript);
    let transcript = strip_terminal_escape_sequences(transcript);
    let response = transcript
        .strip_prefix(previous_transcript.as_str())
        .unwrap_or(transcript.as_str());
    parse_grill_question_group(response)
}

pub fn format_grill_response(answers: &[GrillAnswer]) -> Result<String, DomainError> {
    let mut answers = answers.to_vec();
    answers.sort_by_key(|answer| answer.question_number);
    if answers.is_empty() {
        return Err(DomainError::EmptyGrillAnswer);
    }
    if answers.iter().any(|answer| answer.answer.trim().is_empty()) {
        return Err(DomainError::EmptyGrillAnswer);
    }
    if answers
        .windows(2)
        .any(|pair| pair[0].question_number == pair[1].question_number)
    {
        return Err(DomainError::DuplicateGrillAnswer);
    }
    Ok(answers
        .iter()
        .map(|answer| {
            let mut lines = answer.answer.trim().lines();
            let first = format!(
                "{}. {}",
                answer.question_number,
                lines.next().unwrap_or_default().trim()
            );
            let continuation = lines
                .map(|line| format!("   {}", line.trim()))
                .collect::<Vec<_>>();
            std::iter::once(first)
                .chain(continuation)
                .collect::<Vec<_>>()
                .join("\n")
        })
        .collect::<Vec<_>>()
        .join("\n"))
}

fn parse_question_header(header: &str, fallback_number: u32) -> (u32, Option<String>, String) {
    let header = header.replace("**", "");
    let header = header.trim().trim_start_matches(['-', ':']).trim();
    let (number, rest) = parse_question_number(header).unwrap_or((fallback_number, header));
    let rest = rest.trim();
    let rest = rest
        .strip_prefix('-')
        .or_else(|| rest.strip_prefix(':'))
        .or_else(|| rest.strip_prefix(')'))
        .or_else(|| rest.strip_prefix('.'))
        .unwrap_or(rest)
        .trim();
    let (title, prompt) = rest
        .split_once(':')
        .map(|(title, prompt)| (clean_markup(title), clean_markup(prompt)))
        .filter(|(title, prompt)| !title.is_empty() && !prompt.is_empty())
        .unwrap_or((String::new(), clean_markup(rest)));
    (number, (!title.is_empty()).then_some(title), prompt)
}

fn parse_question_number(value: &str) -> Option<(u32, &str)> {
    let value = value.trim();
    let digit_start = if value
        .chars()
        .next()
        .is_some_and(|character| character.eq_ignore_ascii_case(&'q'))
    {
        1
    } else if value.starts_with("Question") || value.starts_with("question") {
        "Question".len()
    } else {
        0
    };
    let mut end = digit_start;
    for (index, character) in value[digit_start..].char_indices() {
        if !character.is_ascii_digit() {
            break;
        }
        end = digit_start + index + character.len_utf8();
    }
    if end == digit_start {
        return None;
    }
    Some((value[digit_start..end].parse().ok()?, &value[end..]))
}

fn parse_grill_option(value: &str) -> Option<(String, String)> {
    let value = value.trim();
    let (key, rest) = if value.starts_with('(') {
        let closing = value.find(')')?;
        (&value[1..closing], &value[closing + 1..])
    } else {
        let key = value.chars().next()?;
        if !key.is_ascii_uppercase() {
            return None;
        }
        (&value[..key.len_utf8()], &value[key.len_utf8()..])
    };
    if key.len() != 1 || !key.chars().all(|character| character.is_ascii_uppercase()) {
        return None;
    }
    let rest = rest.trim_start();
    let separator = rest.chars().next()?;
    if !matches!(separator, '.' | ')' | ':' | '-') {
        return None;
    }
    let rest = rest[separator.len_utf8()..].trim();
    (!rest.is_empty()).then_some((key.to_owned(), clean_markup(rest)))
}

fn clean_markup(value: &str) -> String {
    value.replace("**", "").replace('*', "").trim().to_owned()
}

fn strip_terminal_escape_sequences(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    let mut characters = value.chars();
    while let Some(character) = characters.next() {
        if character != '\u{1b}' {
            output.push(character);
            continue;
        }
        if characters.next() == Some('[') {
            for character in characters.by_ref() {
                if character.is_ascii_alphabetic() {
                    break;
                }
            }
        }
    }
    output
}

impl Default for GrillConfiguration {
    fn default() -> Self {
        Self {
            agent: AgentKind::Claude,
            model: "claude-sonnet-4-5".into(),
            effort: "high".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GrillEffort {
    pub id: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GrillModel {
    pub id: String,
    pub label: String,
    pub efforts: Vec<GrillEffort>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GrillAgentCatalog {
    pub agent: AgentKind,
    pub models: Vec<GrillModel>,
}

pub fn grill_model_catalog() -> Vec<GrillAgentCatalog> {
    let efforts = |ids: &[(&str, &str)]| {
        ids.iter()
            .map(|(id, label)| GrillEffort {
                id: (*id).into(),
                label: (*label).into(),
            })
            .collect()
    };
    vec![
        GrillAgentCatalog {
            agent: AgentKind::Claude,
            models: vec![
                GrillModel {
                    id: "claude-opus-4-1".into(),
                    label: "Opus".into(),
                    efforts: efforts(&[("low", "Low"), ("medium", "Medium"), ("high", "High")]),
                },
                GrillModel {
                    id: "claude-sonnet-4-5".into(),
                    label: "Sonnet".into(),
                    efforts: efforts(&[("low", "Low"), ("medium", "Medium"), ("high", "High")]),
                },
                GrillModel {
                    id: "claude-haiku-4-5".into(),
                    label: "Haiku".into(),
                    efforts: efforts(&[("low", "Low"), ("medium", "Medium"), ("high", "High")]),
                },
            ],
        },
        GrillAgentCatalog {
            agent: AgentKind::Codex,
            models: vec![
                GrillModel {
                    id: "codex-sol".into(),
                    label: "Sol".into(),
                    efforts: efforts(&[
                        ("low", "Low"),
                        ("medium", "Medium"),
                        ("high", "High"),
                        ("xhigh", "Extra high"),
                    ]),
                },
                GrillModel {
                    id: "codex-terra".into(),
                    label: "Terra".into(),
                    efforts: efforts(&[
                        ("low", "Low"),
                        ("medium", "Medium"),
                        ("high", "High"),
                        ("xhigh", "Extra high"),
                    ]),
                },
                GrillModel {
                    id: "codex-luna".into(),
                    label: "Luna".into(),
                    efforts: efforts(&[
                        ("low", "Low"),
                        ("medium", "Medium"),
                        ("high", "High"),
                        ("xhigh", "Extra high"),
                    ]),
                },
            ],
        },
    ]
}

pub fn validate_grill_configuration(configuration: &GrillConfiguration) -> Result<(), DomainError> {
    let valid = grill_model_catalog()
        .into_iter()
        .find(|catalog| catalog.agent == configuration.agent)
        .and_then(|catalog| {
            catalog
                .models
                .into_iter()
                .find(|model| model.id == configuration.model)
        })
        .is_some_and(|model| {
            model
                .efforts
                .into_iter()
                .any(|effort| effort.id == configuration.effort)
        });
    if valid {
        Ok(())
    } else {
        Err(DomainError::InvalidGrillConfiguration {
            agent: configuration.agent,
            model: configuration.model.clone(),
            effort: configuration.effort.clone(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Context {
    pub id: i64,
    pub name: String,
    pub grill_defaults: GrillConfiguration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectDefaults {
    pub item_status: ItemStatus,
    pub execution_mode: ExecutionMode,
}

impl Default for ProjectDefaults {
    fn default() -> Self {
        Self {
            item_status: ItemStatus::Inbox,
            execution_mode: ExecutionMode::Worktree,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ExecutionMode {
    Direct,
    Worktree,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Project {
    pub id: i64,
    pub context_id: i64,
    pub name: String,
    pub defaults: ProjectDefaults,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Repository {
    pub id: i64,
    pub project_id: i64,
    pub name: String,
    pub remote_url: String,
    pub base_branch: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepositoryLocation {
    pub repository_id: i64,
    pub machine_id: i64,
    pub checkout_path: String,
    pub worktree_root: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceRepositoryInput {
    pub repository_id: i64,
    pub branch: String,
    pub base_branch: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceRepository {
    pub repository_id: i64,
    pub branch: String,
    pub base_branch: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WorkspacePreparationState {
    Pending,
    Resumable,
    Ready,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Workspace {
    pub id: i64,
    pub item_id: i64,
    pub repositories: Vec<WorkspaceRepository>,
    pub preparation_state: WorkspacePreparationState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Worktree {
    pub id: i64,
    pub workspace_id: i64,
    pub repository_id: i64,
    pub machine_id: i64,
    pub path: String,
    pub branch: String,
    pub base_branch: String,
    pub is_dirty: bool,
}

pub fn sanitize_worktree_branch(branch: &str) -> String {
    let mut sanitized = String::new();
    let mut last_was_separator = false;
    for character in branch.trim().chars() {
        let allowed = character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.');
        if allowed {
            sanitized.push(character);
            last_was_separator = false;
        } else if !last_was_separator {
            sanitized.push('-');
            last_was_separator = true;
        }
    }
    let sanitized = sanitized.trim_matches(['-', '.']).to_owned();
    if sanitized.is_empty() || sanitized == "." || sanitized == ".." {
        "branch".into()
    } else {
        sanitized
    }
}

pub fn worktree_path(
    worktree_root: &Path,
    workspace_id: i64,
    branch: &str,
    repository_name: &str,
) -> PathBuf {
    worktree_root
        .join(format!("workspace-{workspace_id}"))
        .join(sanitize_worktree_branch(branch))
        .join(repository_name)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunCheckout {
    pub repository_id: i64,
    pub path: String,
    pub branch: String,
    pub is_dirty: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum MachineTransport {
    #[serde(rename = "local")]
    Local,
    #[serde(rename = "ssh")]
    Ssh {
        host: String,
        user: Option<String>,
        port: Option<u16>,
        identity_file: Option<String>,
        known_hosts_file: Option<String>,
        strict_host_key_checking: Option<String>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MachineObservation {
    #[serde(rename = "unknown")]
    Unknown,
    #[serde(rename = "available")]
    Available,
    #[serde(rename = "offline")]
    Offline,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Machine {
    pub id: i64,
    pub context_id: i64,
    pub name: String,
    pub socket_name: String,
    pub transport: MachineTransport,
    pub last_observed: MachineObservation,
    pub last_observed_at: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentKind {
    #[serde(rename = "claude")]
    Claude,
    #[serde(rename = "codex")]
    Codex,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExecutionProfile {
    #[serde(rename = "investigate")]
    Investigate,
    #[serde(rename = "implement")]
    Implement,
    #[serde(rename = "review")]
    Review,
    #[serde(rename = "custom")]
    CustomPrompt,
    #[serde(rename = "grill")]
    Grill,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RunState {
    #[serde(rename = "unknown")]
    Unknown,
    #[serde(rename = "working")]
    Working,
    #[serde(rename = "blocked")]
    Blocked,
    #[serde(rename = "finished")]
    Finished,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GrillPhase {
    Starting,
    Working,
    WaitingForAnswers,
    AwaitingNextAction,
    RecoverablePaneLoss,
    Finished,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RunPaneStatus {
    #[serde(rename = "unknown")]
    Unknown,
    #[serde(rename = "available")]
    Available,
    #[serde(rename = "missing")]
    Missing,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunPromptSelection {
    pub include_objective: bool,
    pub include_notes: bool,
    pub external_object_ids: Vec<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Run {
    pub id: i64,
    pub item_id: i64,
    pub workspace_id: Option<i64>,
    pub repository_id: Option<i64>,
    pub worktree_id: Option<i64>,
    pub machine_id: i64,
    pub agent: AgentKind,
    pub execution_profile: ExecutionProfile,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub effort: Option<String>,
    #[serde(default)]
    pub skill_snapshot: Option<String>,
    pub prompt: String,
    pub working_directory: String,
    pub session_name: String,
    pub pane_id: String,
    pub started_at: i64,
    pub state: RunState,
    pub pane_status: RunPaneStatus,
    pub direct_checkouts: Vec<RunCheckout>,
    #[serde(default)]
    pub transcript: String,
    #[serde(default)]
    pub grill_question_group: Option<GrillQuestionGroup>,
    #[serde(default)]
    pub grill_answers: Vec<GrillAnswer>,
    #[serde(default)]
    pub grill_decisions: Vec<GrillAnswer>,
    #[serde(default)]
    pub grill_response: Option<String>,
    #[serde(default)]
    pub grill_phase: Option<GrillPhase>,
    #[serde(default)]
    pub grill_action: Option<GrillContinuationAction>,
}

pub fn run_is_active(run: &Run) -> bool {
    run.state != RunState::Finished
        || (run.execution_profile == ExecutionProfile::Grill
            && run.grill_phase != Some(GrillPhase::Finished))
}

pub fn run_is_finished(run: &Run) -> bool {
    !run_is_active(run)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentPaneObservation {
    pub machine_id: i64,
    pub agent: AgentKind,
    pub session_name: String,
    pub pane_id: String,
    pub current_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunSuggestion {
    pub machine_id: i64,
    pub machine_name: String,
    pub agent: AgentKind,
    pub session_name: String,
    pub pane_id: String,
    pub current_path: String,
    pub item_id: i64,
    pub item_identifier: String,
    pub item_title: String,
    pub context_id: i64,
    pub context_name: String,
    #[serde(default)]
    pub workspace_id: Option<i64>,
    #[serde(default)]
    pub repository_id: Option<i64>,
    #[serde(default)]
    pub worktree_id: Option<i64>,
    #[serde(default)]
    pub location_path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reminder {
    pub id: i64,
    pub remind_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Item {
    pub id: i64,
    pub human_identifier: String,
    pub title: String,
    pub project_id: i64,
    pub status: ItemStatus,
    pub notes: String,
    pub reminders: Vec<Reminder>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExternalProvider {
    #[serde(rename = "github")]
    GitHub,
    #[serde(rename = "generic")]
    Generic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExternalObjectKind {
    #[serde(rename = "issue")]
    Issue,
    #[serde(rename = "pull_request")]
    PullRequest,
    #[serde(rename = "generic")]
    Generic,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExternalObjectInput {
    pub provider: ExternalProvider,
    pub kind: ExternalObjectKind,
    pub external_key: String,
    pub canonical_url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExternalObject {
    pub id: i64,
    pub provider: ExternalProvider,
    pub kind: ExternalObjectKind,
    pub external_key: String,
    pub canonical_url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExternalMetadata {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExternalSnapshotData {
    pub title: String,
    pub state: String,
    pub metadata: Vec<ExternalMetadata>,
    pub fetched_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExternalSnapshot {
    pub external_object_id: i64,
    pub title: String,
    pub state: String,
    pub metadata: Vec<ExternalMetadata>,
    pub fetched_at: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExternalChangeKind {
    #[serde(rename = "title")]
    Title,
    #[serde(rename = "state")]
    State,
    #[serde(rename = "metadata")]
    Metadata,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExternalChange {
    pub kind: ExternalChangeKind,
    pub key: Option<String>,
    pub previous: Option<String>,
    pub current: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Activity {
    pub id: i64,
    pub external_object_id: i64,
    pub observed_at: i64,
    pub changes: Vec<ExternalChange>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExternalChangePolicy {
    pub title: bool,
    pub state: bool,
    pub metadata: bool,
}

impl ExternalChangePolicy {
    pub const fn all() -> Self {
        Self {
            title: true,
            state: true,
            metadata: true,
        }
    }

    fn allows(self, kind: ExternalChangeKind) -> bool {
        match kind {
            ExternalChangeKind::Title => self.title,
            ExternalChangeKind::State => self.state,
            ExternalChangeKind::Metadata => self.metadata,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextAttentionDefault {
    pub context_id: i64,
    pub object_kind: ExternalObjectKind,
    pub policy: ExternalChangePolicy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Link {
    pub id: i64,
    pub item_id: i64,
    pub external_object_id: i64,
    pub reviewed_activity_id: i64,
    pub attention_policy: Option<ExternalChangePolicy>,
    pub watch_until: Option<String>,
    pub review_at: Option<String>,
    #[serde(default)]
    pub provenance: Option<LinkProvenance>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AttentionEntryKind {
    #[serde(rename = "external_change")]
    ExternalChange,
    #[serde(rename = "review")]
    Review,
    #[serde(rename = "reminder")]
    Reminder,
    #[serde(rename = "blocked_run")]
    BlockedRun,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttentionEntry {
    pub kind: AttentionEntryKind,
    pub link_id: i64,
    pub reminder_id: Option<i64>,
    pub run_id: Option<i64>,
    pub item_id: i64,
    pub external_object_id: i64,
    pub source_title: String,
    pub source_url: String,
    pub activities: Vec<Activity>,
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExternalLinkView {
    pub link: Link,
    pub object: ExternalObject,
    pub snapshot: Option<ExternalSnapshot>,
    pub attention_policy: ExternalChangePolicy,
    pub attention_entry: Option<AttentionEntry>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ItemStatus {
    Inbox,
    Active,
    Waiting,
    Done,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ItemRelationKind {
    Blocks,
    BlockedBy,
    RelatedTo,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemRelation {
    pub from_item_id: i64,
    pub to_item_id: i64,
    pub kind: ItemRelationKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemView {
    pub item: Item,
    pub context_id: i64,
    pub context_name: String,
    pub project_name: String,
    pub relationships: Vec<ItemRelation>,
    pub workspaces: Vec<Workspace>,
    pub worktrees: Vec<Worktree>,
    pub runs: Vec<Run>,
    pub links: Vec<ExternalLinkView>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemDeletionWorkspace {
    pub id: i64,
    pub item_id: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemDeletionPlan {
    pub item_id: i64,
    pub human_identifier: String,
    pub title: String,
    pub reminder_count: usize,
    pub relationship_count: usize,
    pub workspaces: Vec<ItemDeletionWorkspace>,
    pub run_ids: Vec<i64>,
    pub active_run_ids: Vec<i64>,
    pub link_ids: Vec<i64>,
    pub orphaned_external_object_ids: Vec<i64>,
    pub orphaned_snapshot_count: usize,
    pub orphaned_activity_count: usize,
    #[serde(skip)]
    pub state_fingerprint: String,
}

impl ItemDeletionPlan {
    pub fn summary(&self) -> ItemDeletionSummary {
        ItemDeletionSummary {
            item_id: self.item_id,
            reminder_count: self.reminder_count,
            relationship_count: self.relationship_count,
            workspace_count: self.workspaces.len(),
            run_count: self.run_ids.len(),
            link_count: self.link_ids.len(),
            external_object_count: self.orphaned_external_object_ids.len(),
            snapshot_count: self.orphaned_snapshot_count,
            activity_count: self.orphaned_activity_count,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemDeletionSummary {
    pub item_id: i64,
    pub reminder_count: usize,
    pub relationship_count: usize,
    pub workspace_count: usize,
    pub run_count: usize,
    pub link_count: usize,
    pub external_object_count: usize,
    pub snapshot_count: usize,
    pub activity_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalObjectDeletionPlan {
    pub external_object_id: i64,
    pub provider: ExternalProvider,
    pub kind: ExternalObjectKind,
    pub external_key: String,
    pub canonical_url: String,
    pub link_ids: Vec<i64>,
    pub snapshot_count: usize,
    pub activity_count: usize,
    #[serde(skip)]
    pub state_fingerprint: String,
}

impl ExternalObjectDeletionPlan {
    pub fn summary(&self) -> ExternalObjectDeletionSummary {
        ExternalObjectDeletionSummary {
            external_object_id: self.external_object_id,
            link_count: self.link_ids.len(),
            snapshot_count: self.snapshot_count,
            activity_count: self.activity_count,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalObjectDeletionSummary {
    pub external_object_id: i64,
    pub link_count: usize,
    pub snapshot_count: usize,
    pub activity_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepositoryDeletionWorkspace {
    pub id: i64,
    pub item_id: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepositoryDeletionPlan {
    pub repository_id: i64,
    pub name: String,
    pub remote_url: String,
    pub workspaces: Vec<RepositoryDeletionWorkspace>,
    #[serde(skip)]
    pub state_fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MachineDeletionRun {
    pub id: i64,
    pub item_id: i64,
    pub item_identifier: String,
    pub item_title: String,
    pub workspace_id: Option<i64>,
    pub worktree_id: Option<i64>,
    pub state: RunState,
    pub pane_status: RunPaneStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MachineDeletionPlan {
    pub machine_id: i64,
    pub name: String,
    pub runs: Vec<MachineDeletionRun>,
    pub active_run_ids: Vec<i64>,
    #[serde(skip)]
    pub state_fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParentDeletionProject {
    pub id: i64,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParentDeletionItem {
    pub id: i64,
    pub human_identifier: String,
    pub title: String,
    pub project_id: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParentDeletionRepository {
    pub id: i64,
    pub name: String,
    pub remote_url: String,
    pub project_id: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParentDeletionMachine {
    pub id: i64,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParentDeletionRun {
    pub id: i64,
    pub item_id: i64,
    pub item_identifier: String,
    pub item_title: String,
    pub workspace_id: Option<i64>,
    pub worktree_id: Option<i64>,
    pub machine_id: i64,
    pub state: RunState,
    pub pane_status: RunPaneStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParentDeletionSummary {
    pub context_id: Option<i64>,
    pub project_id: Option<i64>,
    pub project_count: usize,
    pub item_count: usize,
    pub repository_count: usize,
    pub machine_count: usize,
    pub workspace_count: usize,
    pub run_count: usize,
    pub reminder_count: usize,
    pub relationship_count: usize,
    pub link_count: usize,
    pub attention_default_count: usize,
    pub external_object_count: usize,
    pub snapshot_count: usize,
    pub activity_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResetLocalDataSummary {
    pub context_count: usize,
    pub project_count: usize,
    pub repository_count: usize,
    pub item_count: usize,
    pub workspace_count: usize,
    pub machine_count: usize,
    pub run_count: usize,
    pub reminder_count: usize,
    pub relationship_count: usize,
    pub link_count: usize,
    pub external_object_count: usize,
    pub snapshot_count: usize,
    pub activity_count: usize,
    pub attention_default_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResetLocalDataRecord {
    pub kind: String,
    pub id: i64,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResetLocalDataPlan {
    pub summary: ResetLocalDataSummary,
    pub affected_records: Vec<ResetLocalDataRecord>,
    pub workspaces: Vec<ItemDeletionWorkspace>,
    #[serde(skip)]
    pub state_fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParentDeletionPlan {
    pub context_id: Option<i64>,
    pub project_id: Option<i64>,
    pub name: String,
    pub projects: Vec<ParentDeletionProject>,
    pub items: Vec<ParentDeletionItem>,
    pub repositories: Vec<ParentDeletionRepository>,
    pub machines: Vec<ParentDeletionMachine>,
    pub workspaces: Vec<ItemDeletionWorkspace>,
    pub runs: Vec<ParentDeletionRun>,
    pub active_run_ids: Vec<i64>,
    pub reminder_count: usize,
    pub relationship_count: usize,
    pub link_ids: Vec<i64>,
    pub attention_defaults: Vec<ContextAttentionDefault>,
    pub orphaned_external_object_ids: Vec<i64>,
    pub orphaned_snapshot_count: usize,
    pub orphaned_activity_count: usize,
    #[serde(skip)]
    pub state_fingerprint: String,
}

impl ParentDeletionPlan {
    pub fn summary(&self) -> ParentDeletionSummary {
        ParentDeletionSummary {
            context_id: self.context_id,
            project_id: self.project_id,
            project_count: self.projects.len(),
            item_count: self.items.len(),
            repository_count: self.repositories.len(),
            machine_count: self.machines.len(),
            workspace_count: self.workspaces.len(),
            run_count: self.runs.len(),
            reminder_count: self.reminder_count,
            relationship_count: self.relationship_count,
            link_count: self.link_ids.len(),
            attention_default_count: self.attention_defaults.len(),
            external_object_count: self.orphaned_external_object_ids.len(),
            snapshot_count: self.orphaned_snapshot_count,
            activity_count: self.orphaned_activity_count,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HomeView {
    pub needs_attention: Vec<ItemView>,
    pub attention_entries: Vec<AttentionEntry>,
    pub running: Vec<ItemView>,
    pub waiting: Vec<ItemView>,
    pub due: Vec<ItemView>,
    pub completed: Vec<ItemView>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "camelCase")]
pub enum AuditAction {
    ContextCreated {
        context_id: i64,
    },
    ProjectCreated {
        project_id: i64,
    },
    RepositoryRegistered {
        repository_id: i64,
    },
    ItemCreated {
        item_id: i64,
    },
    ItemStatusChanged {
        item_id: i64,
        from: ItemStatus,
        to: ItemStatus,
    },
    ItemTitleChanged {
        item_id: i64,
    },
    ItemNotesChanged {
        item_id: i64,
    },
    ItemRemindersChanged {
        item_id: i64,
    },
    ItemRelationChanged {
        from_item_id: i64,
        to_item_id: i64,
        kind: ItemRelationKind,
    },
    WorkspaceCreated {
        workspace_id: i64,
    },
    WorkspaceUpdated {
        workspace_id: i64,
    },
    WorkspaceRemoved {
        workspace_id: i64,
        #[serde(default)]
        repository_count: Option<usize>,
    },
    MachineRegistered {
        machine_id: i64,
    },
    MachineObserved {
        machine_id: i64,
        observation: MachineObservation,
    },
    MachineDeleted {
        machine_id: i64,
        #[serde(default)]
        run_count: Option<usize>,
    },
    RunCreated {
        run_id: i64,
    },
    RunStopped {
        run_id: i64,
    },
    RunFinished {
        run_id: i64,
    },
    RunStateChanged {
        run_id: i64,
        from: RunState,
        to: RunState,
    },
    RunDeleted {
        run_id: i64,
    },
    RunPaneStatusChanged {
        run_id: i64,
        from: RunPaneStatus,
        to: RunPaneStatus,
    },
    ExternalObjectCreated {
        external_object_id: i64,
    },
    ExternalObjectRefreshed {
        external_object_id: i64,
    },
    LinkCreated {
        link_id: i64,
    },
    LinkUpdated {
        link_id: i64,
    },
    LinkDeleted {
        link_id: i64,
        #[serde(default)]
        external_object_id: Option<i64>,
        #[serde(default)]
        external_object_deleted: Option<bool>,
    },
    ExternalObjectDeleted {
        external_object_id: i64,
        #[serde(default)]
        link_count: Option<usize>,
        #[serde(default)]
        snapshot_count: Option<usize>,
        #[serde(default)]
        activity_count: Option<usize>,
    },
    ContextAttentionDefaultChanged {
        context_id: i64,
        object_kind: ExternalObjectKind,
    },
    ItemDeleted {
        summary: ItemDeletionSummary,
    },
    RepositoryDeleted {
        repository_id: i64,
        #[serde(default)]
        workspace_count: Option<usize>,
    },
    ProjectDeleted {
        summary: ParentDeletionSummary,
    },
    ContextDeleted {
        summary: ParentDeletionSummary,
    },
    ResetBoundary {
        context_id: i64,
        project_id: i64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditEntry {
    pub id: i64,
    pub recorded_at: i64,
    pub action: AuditAction,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObservedActivity {
    pub activity: Activity,
    pub object: ExternalObject,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActivityTabView {
    pub audit_entries: Vec<AuditEntry>,
    pub activities: Vec<ObservedActivity>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DomainState {
    pub next_context_id: i64,
    pub next_project_id: i64,
    pub next_item_id: i64,
    pub next_item_number: i64,
    pub next_repository_id: i64,
    pub next_workspace_id: i64,
    pub next_worktree_id: i64,
    pub next_machine_id: i64,
    pub next_run_id: i64,
    pub next_external_object_id: i64,
    pub next_link_id: i64,
    pub next_activity_id: i64,
    pub next_reminder_id: i64,
    pub contexts: Vec<Context>,
    pub projects: Vec<Project>,
    pub repositories: Vec<Repository>,
    pub repository_locations: Vec<RepositoryLocation>,
    pub items: Vec<Item>,
    pub workspaces: Vec<Workspace>,
    pub worktrees: Vec<Worktree>,
    pub machines: Vec<Machine>,
    pub runs: Vec<Run>,
    pub relationships: Vec<ItemRelation>,
    pub external_objects: Vec<ExternalObject>,
    pub links: Vec<Link>,
    pub snapshots: Vec<ExternalSnapshot>,
    pub activities: Vec<Activity>,
    pub attention_defaults: Vec<ContextAttentionDefault>,
}

pub fn activity_tab_view(state: &DomainState, audit_entries: Vec<AuditEntry>) -> ActivityTabView {
    let activities = state
        .activities
        .iter()
        .rev()
        .filter_map(|activity| {
            let object = state
                .external_objects
                .iter()
                .find(|object| object.id == activity.external_object_id)?;
            Some(ObservedActivity {
                activity: activity.clone(),
                object: object.clone(),
            })
        })
        .collect();

    ActivityTabView {
        audit_entries,
        activities,
    }
}

pub fn plan_reset_local_data(state: &DomainState) -> ResetLocalDataPlan {
    let mut affected_records = Vec::new();
    affected_records.extend(state.contexts.iter().map(|context| ResetLocalDataRecord {
        kind: "Context".into(),
        id: context.id,
        label: context.name.clone(),
    }));
    affected_records.extend(state.projects.iter().map(|project| ResetLocalDataRecord {
        kind: "Project".into(),
        id: project.id,
        label: project.name.clone(),
    }));
    affected_records.extend(
        state
            .repositories
            .iter()
            .map(|repository| ResetLocalDataRecord {
                kind: "Repository".into(),
                id: repository.id,
                label: repository.name.clone(),
            }),
    );
    affected_records.extend(state.items.iter().map(|item| ResetLocalDataRecord {
        kind: "Item".into(),
        id: item.id,
        label: format!("{} · {}", item.human_identifier, item.title),
    }));
    affected_records.extend(
        state
            .workspaces
            .iter()
            .map(|workspace| ResetLocalDataRecord {
                kind: "Workspace".into(),
                id: workspace.id,
                label: format!("Item {}", workspace.item_id),
            }),
    );
    affected_records.extend(state.machines.iter().map(|machine| ResetLocalDataRecord {
        kind: "Machine".into(),
        id: machine.id,
        label: machine.name.clone(),
    }));
    affected_records.extend(state.runs.iter().map(|run| ResetLocalDataRecord {
        kind: "Run".into(),
        id: run.id,
        label: format!(
            "{:?} · Item {} · Machine {}",
            run.state,
            state
                .items
                .iter()
                .find(|item| item.id == run.item_id)
                .map(|item| item.human_identifier.as_str())
                .unwrap_or("unknown"),
            state
                .machines
                .iter()
                .find(|machine| machine.id == run.machine_id)
                .map(|machine| machine.name.as_str())
                .unwrap_or("unknown")
        ),
    }));
    affected_records.extend(state.links.iter().map(|link| ResetLocalDataRecord {
        kind: "Link".into(),
        id: link.id,
        label: format!(
            "Item {} → {}",
            state
                .items
                .iter()
                .find(|item| item.id == link.item_id)
                .map(|item| item.human_identifier.as_str())
                .unwrap_or("unknown"),
            state
                .external_objects
                .iter()
                .find(|object| object.id == link.external_object_id)
                .map(|object| object.external_key.as_str())
                .unwrap_or("unknown")
        ),
    }));
    affected_records.extend(state.external_objects.iter().map(|external_object| {
        ResetLocalDataRecord {
            kind: "External Object".into(),
            id: external_object.id,
            label: external_object.external_key.clone(),
        }
    }));

    ResetLocalDataPlan {
        summary: ResetLocalDataSummary {
            context_count: state.contexts.len(),
            project_count: state.projects.len(),
            repository_count: state.repositories.len(),
            item_count: state.items.len(),
            workspace_count: state.workspaces.len(),
            machine_count: state.machines.len(),
            run_count: state.runs.len(),
            reminder_count: state.items.iter().map(|item| item.reminders.len()).sum(),
            relationship_count: state.relationships.len(),
            link_count: state.links.len(),
            external_object_count: state.external_objects.len(),
            snapshot_count: state.snapshots.len(),
            activity_count: state.activities.len(),
            attention_default_count: state.attention_defaults.len(),
        },
        affected_records,
        workspaces: state
            .workspaces
            .iter()
            .map(|workspace| ItemDeletionWorkspace {
                id: workspace.id,
                item_id: workspace.item_id,
            })
            .collect(),
        state_fingerprint: serde_json::to_string(state)
            .expect("DomainState should always be serializable"),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    CreateContext {
        name: String,
    },
    SetContextGrillDefaults {
        context_id: i64,
        defaults: GrillConfiguration,
    },
    CreateProject {
        context_id: i64,
        name: String,
        defaults: ProjectDefaults,
    },
    RegisterRepository {
        project_id: i64,
        name: String,
        remote_url: String,
    },
    RegisterRepositoryAtLocation {
        project_id: i64,
        name: String,
        remote_url: String,
        base_branch: String,
        machine_id: i64,
        checkout_path: String,
        worktree_root: String,
    },
    ConfigureRepositoryLocation {
        repository_id: i64,
        machine_id: i64,
        checkout_path: String,
        worktree_root: String,
    },
    ResetLocalData,
    DeleteRepository {
        repository_id: i64,
        workspace_ids: Vec<i64>,
    },
    DeleteProject {
        project_id: i64,
        item_ids: Vec<i64>,
        repository_ids: Vec<i64>,
        workspace_ids: Vec<i64>,
    },
    DeleteContext {
        context_id: i64,
        project_ids: Vec<i64>,
        item_ids: Vec<i64>,
        repository_ids: Vec<i64>,
        workspace_ids: Vec<i64>,
        machine_ids: Vec<i64>,
    },
    DeleteMachine {
        machine_id: i64,
        run_ids: Vec<i64>,
    },
    CreateItem {
        title: String,
        context_id: i64,
        project_id: i64,
    },
    CreateWorkspace {
        item_id: i64,
        repositories: Vec<WorkspaceRepositoryInput>,
    },
    CreateWorktree {
        workspace_id: i64,
        repository_id: i64,
        machine_id: i64,
        path: String,
        branch: String,
        base_branch: String,
        is_dirty: bool,
    },
    MarkWorkspaceResumable {
        workspace_id: i64,
    },
    RemoveWorktree {
        worktree_id: i64,
    },
    RemoveWorkspace {
        workspace_id: i64,
    },
    RegisterMachine {
        context_id: i64,
        name: String,
        socket_name: String,
        transport: MachineTransport,
    },
    ObserveMachine {
        machine_id: i64,
        observation: MachineObservation,
        observed_at: i64,
    },
    DeleteItem {
        item_id: i64,
    },
    DeleteLink {
        link_id: i64,
    },
    DeleteExternalObject {
        external_object_id: i64,
    },
    DeleteRun {
        run_id: i64,
    },
    StartDirectRun {
        item_id: i64,
        workspace_id: i64,
        machine_id: i64,
        agent: AgentKind,
        execution_profile: ExecutionProfile,
        prompt: String,
        working_directory: String,
        session_name: String,
        pane_id: String,
        started_at: i64,
        prompt_selection: RunPromptSelection,
        checkouts: Vec<RunCheckout>,
        repository_id: i64,
        allow_dirty: bool,
        allow_shared_checkouts: bool,
    },
    StartWorktreeRun {
        item_id: i64,
        workspace_id: i64,
        worktree_id: i64,
        machine_id: i64,
        agent: AgentKind,
        execution_profile: ExecutionProfile,
        prompt: String,
        working_directory: String,
        session_name: String,
        pane_id: String,
        started_at: i64,
        prompt_selection: RunPromptSelection,
    },
    StartGrillRun {
        item_id: i64,
        workspace_id: i64,
        repository_id: i64,
        machine_id: i64,
        configuration: GrillConfiguration,
        prompt: String,
        skill_snapshot: String,
        working_directory: String,
        session_name: String,
        pane_id: String,
        started_at: i64,
        checkouts: Vec<RunCheckout>,
    },
    AttachRun {
        item_id: i64,
        workspace_id: i64,
        worktree_id: Option<i64>,
        repository_id: i64,
        machine_id: i64,
        agent: AgentKind,
        working_directory: String,
        session_name: String,
        pane_id: String,
        attached_at: i64,
    },
    UpdateRunState {
        run_id: i64,
        state: RunState,
    },
    FinishRun {
        run_id: i64,
    },
    ContinueGrill {
        run_id: i64,
        action: GrillContinuationAction,
    },
    CaptureDownstreamIssues {
        run_id: i64,
        action: GrillContinuationAction,
        issues: Vec<ConfirmedDownstreamIssue>,
    },
    RecordRunTranscript {
        run_id: i64,
        transcript: String,
        question_group: Option<GrillQuestionGroup>,
    },
    RecordGrillAnswers {
        run_id: i64,
        answers: Vec<GrillAnswer>,
    },
    RecordGrillResponse {
        run_id: i64,
        response: String,
    },
    SetRunPaneStatus {
        run_id: i64,
        status: RunPaneStatus,
    },
    SetItemStatus {
        item_id: i64,
        status: ItemStatus,
    },
    SetItemTitle {
        item_id: i64,
        title: String,
    },
    SetItemNotes {
        item_id: i64,
        notes: String,
    },
    SetItemRelation {
        from_item_id: i64,
        to_item_id: i64,
        kind: ItemRelationKind,
    },
    AddItemReminder {
        item_id: i64,
        remind_at: String,
    },
    RemoveItemReminder {
        item_id: i64,
        reminder_id: i64,
    },
    SetLinkWatchUntil {
        link_id: i64,
        watch_until: Option<String>,
    },
    SetLinkReviewAt {
        link_id: i64,
        review_at: Option<String>,
    },
    ClearLinkReviewAt {
        link_id: i64,
    },
    LinkExternalObject {
        item_id: i64,
        object: ExternalObjectInput,
        snapshot: Option<ExternalSnapshotData>,
    },
    RefreshExternalObject {
        external_object_id: i64,
        snapshot: ExternalSnapshotData,
    },
    SetLinkAttentionPolicy {
        link_id: i64,
        policy: Option<ExternalChangePolicy>,
    },
    SetContextAttentionDefault {
        context_id: i64,
        object_kind: ExternalObjectKind,
        policy: ExternalChangePolicy,
    },
    MarkLinkReviewed {
        link_id: i64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    PersistContext {
        context: Context,
        next_context_id: i64,
    },
    PersistContextGrillDefaults {
        context: Context,
    },
    PersistProject {
        project: Project,
        next_project_id: i64,
    },
    PersistRepository {
        repository: Repository,
        next_repository_id: i64,
    },
    UpdateRepository {
        repository: Repository,
    },
    PersistRepositoryLocation {
        location: RepositoryLocation,
    },
    ResetLocalData {
        context: Context,
        project: Project,
        next_context_id: i64,
        next_project_id: i64,
    },
    PersistItem {
        item: Item,
        next_item_number: i64,
        next_item_id: i64,
    },
    PersistWorkspace {
        workspace: Workspace,
        next_workspace_id: i64,
    },
    PersistWorkspaceUpdate {
        workspace: Workspace,
    },
    PersistWorktree {
        worktree: Worktree,
        next_worktree_id: i64,
    },
    RemoveWorktree {
        worktree_id: i64,
    },
    RemoveWorkspace {
        workspace_id: i64,
    },
    RemoveRepository {
        repository_id: i64,
    },
    RemoveMachine {
        machine_id: i64,
    },
    PersistMachine {
        machine: Machine,
        next_machine_id: i64,
    },
    PersistMachineObservation {
        machine: Machine,
    },
    PersistRun {
        run: Run,
        next_run_id: i64,
    },
    PersistRunState {
        run: Run,
    },
    PersistRunTranscript {
        run: Run,
    },
    PersistGrillAnswers {
        run: Run,
    },
    PersistGrillResponse {
        run: Run,
    },
    PersistRunPaneStatus {
        run: Run,
    },
    RemoveRun {
        run_id: i64,
    },
    PersistItemUpdate {
        item: Item,
    },
    PersistItemReminders {
        item: Item,
        next_reminder_id: i64,
    },
    PersistItemRelation {
        relation: ItemRelation,
    },
    PersistExternalObject {
        object: ExternalObject,
        next_external_object_id: i64,
    },
    PersistLink {
        link: Link,
        next_link_id: i64,
    },
    PersistLinkState {
        link: Link,
    },
    PersistExternalSnapshot {
        snapshot: ExternalSnapshot,
    },
    PersistActivity {
        activity: Activity,
        next_activity_id: i64,
    },
    PersistContextAttentionDefault {
        attention_default: ContextAttentionDefault,
    },
    RemoveItemCascade {
        item_id: i64,
        orphaned_external_object_ids: Vec<i64>,
        summary: ItemDeletionSummary,
    },
    RemoveProjectCascade {
        project_id: i64,
        item_ids: Vec<i64>,
        repository_ids: Vec<i64>,
        workspace_ids: Vec<i64>,
        orphaned_external_object_ids: Vec<i64>,
        summary: ParentDeletionSummary,
    },
    RemoveContextCascade {
        context_id: i64,
        project_ids: Vec<i64>,
        item_ids: Vec<i64>,
        repository_ids: Vec<i64>,
        workspace_ids: Vec<i64>,
        machine_ids: Vec<i64>,
        orphaned_external_object_ids: Vec<i64>,
        summary: ParentDeletionSummary,
    },
    RemoveLink {
        link_id: i64,
        external_object_id: i64,
    },
    RemoveExternalObject {
        external_object_id: i64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decision {
    pub state: DomainState,
    pub effects: Vec<Effect>,
}

pub fn plan_item_deletion(
    state: &DomainState,
    item_id: i64,
) -> Result<ItemDeletionPlan, DomainError> {
    let item = state
        .items
        .iter()
        .find(|item| item.id == item_id)
        .ok_or(DomainError::ItemNotFound { item_id })?;
    let workspaces = state
        .workspaces
        .iter()
        .filter(|workspace| workspace.item_id == item_id)
        .map(|workspace| ItemDeletionWorkspace {
            id: workspace.id,
            item_id: workspace.item_id,
        })
        .collect::<Vec<_>>();
    let run_ids = state
        .runs
        .iter()
        .filter(|run| run.item_id == item_id)
        .map(|run| run.id)
        .collect::<Vec<_>>();
    let active_run_ids = state
        .runs
        .iter()
        .filter(|run| run.item_id == item_id && run_is_active(run))
        .map(|run| run.id)
        .collect::<Vec<_>>();
    let link_ids = state
        .links
        .iter()
        .filter(|link| link.item_id == item_id)
        .map(|link| link.id)
        .collect::<Vec<_>>();
    let linked_external_object_ids = state
        .links
        .iter()
        .filter(|link| link.item_id == item_id)
        .map(|link| link.external_object_id)
        .collect::<Vec<_>>();
    let orphaned_external_object_ids = linked_external_object_ids
        .iter()
        .copied()
        .filter(|external_object_id| {
            !state.links.iter().any(|link| {
                link.external_object_id == *external_object_id && link.item_id != item_id
            })
        })
        .collect::<Vec<_>>();

    Ok(ItemDeletionPlan {
        item_id,
        human_identifier: item.human_identifier.clone(),
        title: item.title.clone(),
        reminder_count: item.reminders.len(),
        relationship_count: state
            .relationships
            .iter()
            .filter(|relation| relation.from_item_id == item_id || relation.to_item_id == item_id)
            .count(),
        workspaces,
        run_ids,
        active_run_ids,
        link_ids,
        orphaned_snapshot_count: state
            .snapshots
            .iter()
            .filter(|snapshot| orphaned_external_object_ids.contains(&snapshot.external_object_id))
            .count(),
        orphaned_activity_count: state
            .activities
            .iter()
            .filter(|activity| orphaned_external_object_ids.contains(&activity.external_object_id))
            .count(),
        orphaned_external_object_ids,
        state_fingerprint: serde_json::to_string(state)
            .expect("DomainState should always be serializable"),
    })
}

pub fn plan_external_object_deletion(
    state: &DomainState,
    external_object_id: i64,
) -> Result<ExternalObjectDeletionPlan, DomainError> {
    let object = state
        .external_objects
        .iter()
        .find(|object| object.id == external_object_id)
        .ok_or(DomainError::ExternalObjectNotFound { external_object_id })?;
    let link_ids = state
        .links
        .iter()
        .filter(|link| link.external_object_id == external_object_id)
        .map(|link| link.id)
        .collect::<Vec<_>>();

    Ok(ExternalObjectDeletionPlan {
        external_object_id,
        provider: object.provider,
        kind: object.kind,
        external_key: object.external_key.clone(),
        canonical_url: object.canonical_url.clone(),
        link_ids,
        snapshot_count: state
            .snapshots
            .iter()
            .filter(|snapshot| snapshot.external_object_id == external_object_id)
            .count(),
        activity_count: state
            .activities
            .iter()
            .filter(|activity| activity.external_object_id == external_object_id)
            .count(),
        state_fingerprint: serde_json::to_string(state)
            .expect("DomainState should always be serializable"),
    })
}

pub fn plan_repository_deletion(
    state: &DomainState,
    repository_id: i64,
) -> Result<RepositoryDeletionPlan, DomainError> {
    let repository = state
        .repositories
        .iter()
        .find(|repository| repository.id == repository_id)
        .ok_or(DomainError::RepositoryNotFound { repository_id })?;
    let workspaces = state
        .workspaces
        .iter()
        .filter(|workspace| {
            workspace
                .repositories
                .iter()
                .any(|selected| selected.repository_id == repository_id)
        })
        .map(|workspace| RepositoryDeletionWorkspace {
            id: workspace.id,
            item_id: workspace.item_id,
        })
        .collect::<Vec<_>>();

    Ok(RepositoryDeletionPlan {
        repository_id,
        name: repository.name.clone(),
        remote_url: repository.remote_url.clone(),
        workspaces,
        state_fingerprint: serde_json::to_string(state)
            .expect("DomainState should always be serializable"),
    })
}

pub fn plan_machine_deletion(
    state: &DomainState,
    machine_id: i64,
) -> Result<MachineDeletionPlan, DomainError> {
    let machine = state
        .machines
        .iter()
        .find(|machine| machine.id == machine_id)
        .ok_or(DomainError::MachineNotFound { machine_id })?;
    let runs = state
        .runs
        .iter()
        .filter(|run| run.machine_id == machine_id)
        .map(|run| {
            let item = state
                .items
                .iter()
                .find(|item| item.id == run.item_id)
                .ok_or(DomainError::ItemNotFound {
                    item_id: run.item_id,
                })?;
            Ok(MachineDeletionRun {
                id: run.id,
                item_id: run.item_id,
                item_identifier: item.human_identifier.clone(),
                item_title: item.title.clone(),
                workspace_id: run.workspace_id,
                worktree_id: run.worktree_id,
                state: run.state,
                pane_status: run.pane_status,
            })
        })
        .collect::<Result<Vec<_>, DomainError>>()?;
    let active_run_ids = state
        .runs
        .iter()
        .filter(|run| run.machine_id == machine_id && run_is_active(run))
        .map(|run| run.id)
        .collect();

    Ok(MachineDeletionPlan {
        machine_id,
        name: machine.name.clone(),
        runs,
        active_run_ids,
        state_fingerprint: serde_json::to_string(state)
            .expect("DomainState should always be serializable"),
    })
}

pub fn plan_project_deletion(
    state: &DomainState,
    project_id: i64,
) -> Result<ParentDeletionPlan, DomainError> {
    let project = state
        .projects
        .iter()
        .find(|project| project.id == project_id)
        .ok_or(DomainError::ProjectNotFound { project_id })?;
    plan_parent_deletion(
        state,
        None,
        Some(project_id),
        vec![project_id],
        project.name.clone(),
    )
}

pub fn plan_context_deletion(
    state: &DomainState,
    context_id: i64,
) -> Result<ParentDeletionPlan, DomainError> {
    let context = state
        .contexts
        .iter()
        .find(|context| context.id == context_id)
        .ok_or(DomainError::ContextNotFound { context_id })?;
    let project_ids = state
        .projects
        .iter()
        .filter(|project| project.context_id == context_id)
        .map(|project| project.id)
        .collect();
    plan_parent_deletion(
        state,
        Some(context_id),
        None,
        project_ids,
        context.name.clone(),
    )
}

fn plan_parent_deletion(
    state: &DomainState,
    context_id: Option<i64>,
    project_id: Option<i64>,
    project_ids: Vec<i64>,
    name: String,
) -> Result<ParentDeletionPlan, DomainError> {
    let projects = state
        .projects
        .iter()
        .filter(|project| project_ids.contains(&project.id))
        .map(|project| ParentDeletionProject {
            id: project.id,
            name: project.name.clone(),
        })
        .collect::<Vec<_>>();
    let items = state
        .items
        .iter()
        .filter(|item| project_ids.contains(&item.project_id))
        .map(|item| ParentDeletionItem {
            id: item.id,
            human_identifier: item.human_identifier.clone(),
            title: item.title.clone(),
            project_id: item.project_id,
        })
        .collect::<Vec<_>>();
    let item_ids = items.iter().map(|item| item.id).collect::<Vec<_>>();
    let repositories = state
        .repositories
        .iter()
        .filter(|repository| project_ids.contains(&repository.project_id))
        .map(|repository| ParentDeletionRepository {
            id: repository.id,
            name: repository.name.clone(),
            remote_url: repository.remote_url.clone(),
            project_id: repository.project_id,
        })
        .collect::<Vec<_>>();
    let workspaces = state
        .workspaces
        .iter()
        .filter(|workspace| item_ids.contains(&workspace.item_id))
        .map(|workspace| ItemDeletionWorkspace {
            id: workspace.id,
            item_id: workspace.item_id,
        })
        .collect::<Vec<_>>();
    let machines = context_id
        .map(|context_id| {
            state
                .machines
                .iter()
                .filter(|machine| machine.context_id == context_id)
                .map(|machine| ParentDeletionMachine {
                    id: machine.id,
                    name: machine.name.clone(),
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let machine_ids = machines
        .iter()
        .map(|machine| machine.id)
        .collect::<Vec<_>>();
    let runs = state
        .runs
        .iter()
        .filter(|run| item_ids.contains(&run.item_id) || machine_ids.contains(&run.machine_id))
        .map(|run| {
            let item = state
                .items
                .iter()
                .find(|item| item.id == run.item_id)
                .ok_or(DomainError::ItemNotFound {
                    item_id: run.item_id,
                })?;
            Ok(ParentDeletionRun {
                id: run.id,
                item_id: run.item_id,
                item_identifier: item.human_identifier.clone(),
                item_title: item.title.clone(),
                workspace_id: run.workspace_id,
                worktree_id: run.worktree_id,
                machine_id: run.machine_id,
                state: run.state,
                pane_status: run.pane_status,
            })
        })
        .collect::<Result<Vec<_>, DomainError>>()?;
    let active_run_ids = state
        .runs
        .iter()
        .filter(|run| {
            (item_ids.contains(&run.item_id) || machine_ids.contains(&run.machine_id))
                && run_is_active(run)
        })
        .map(|run| run.id)
        .collect::<Vec<_>>();
    let link_ids = state
        .links
        .iter()
        .filter(|link| item_ids.contains(&link.item_id))
        .map(|link| link.id)
        .collect::<Vec<_>>();
    let linked_external_object_ids = state
        .links
        .iter()
        .filter(|link| item_ids.contains(&link.item_id))
        .map(|link| link.external_object_id)
        .collect::<Vec<_>>();
    let orphaned_external_object_ids = linked_external_object_ids
        .iter()
        .copied()
        .filter(|external_object_id| {
            !state.links.iter().any(|link| {
                link.external_object_id == *external_object_id && !item_ids.contains(&link.item_id)
            })
        })
        .fold(Vec::new(), |mut ids, external_object_id| {
            if !ids.contains(&external_object_id) {
                ids.push(external_object_id);
            }
            ids
        });
    let attention_defaults = context_id
        .map(|context_id| {
            state
                .attention_defaults
                .iter()
                .filter(|attention_default| attention_default.context_id == context_id)
                .cloned()
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    Ok(ParentDeletionPlan {
        context_id,
        project_id,
        name,
        projects,
        items: items.clone(),
        repositories,
        machines,
        workspaces,
        runs,
        active_run_ids,
        reminder_count: items
            .iter()
            .filter_map(|parent_item| state.items.iter().find(|item| item.id == parent_item.id))
            .map(|item| item.reminders.len())
            .sum(),
        relationship_count: state
            .relationships
            .iter()
            .filter(|relation| {
                item_ids.contains(&relation.from_item_id) || item_ids.contains(&relation.to_item_id)
            })
            .count(),
        link_ids,
        attention_defaults,
        orphaned_snapshot_count: state
            .snapshots
            .iter()
            .filter(|snapshot| orphaned_external_object_ids.contains(&snapshot.external_object_id))
            .count(),
        orphaned_activity_count: state
            .activities
            .iter()
            .filter(|activity| orphaned_external_object_ids.contains(&activity.external_object_id))
            .count(),
        orphaned_external_object_ids,
        state_fingerprint: serde_json::to_string(state)
            .expect("DomainState should always be serializable"),
    })
}

fn sorted_ids(mut ids: Vec<i64>) -> Vec<i64> {
    ids.sort_unstable();
    ids
}

fn parent_selection_matches(expected: &[i64], provided: Vec<i64>) -> bool {
    sorted_ids(expected.to_vec()) == sorted_ids(provided)
}

fn workspace_preparation_state(
    state: &DomainState,
    workspace_id: i64,
    previous: WorkspacePreparationState,
) -> WorkspacePreparationState {
    let Some(workspace) = state
        .workspaces
        .iter()
        .find(|workspace| workspace.id == workspace_id)
    else {
        return previous;
    };
    let complete = workspace.repositories.iter().all(|repository| {
        state.worktrees.iter().any(|worktree| {
            worktree.workspace_id == workspace_id
                && worktree.repository_id == repository.repository_id
        })
    });
    if complete {
        WorkspacePreparationState::Ready
    } else if previous == WorkspacePreparationState::Resumable {
        WorkspacePreparationState::Resumable
    } else {
        WorkspacePreparationState::Pending
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum DomainError {
    #[error("a Context name cannot be blank")]
    EmptyContextName,
    #[error("a Project name cannot be blank")]
    EmptyProjectName,
    #[error("an Item title cannot be blank")]
    EmptyTitle,
    #[error("a Reminder date cannot be blank")]
    EmptyReminderAt,
    #[error("Context name already exists: {name}")]
    ContextNameTaken { name: String },
    #[error("Project name already exists in Context {context_id}: {name}")]
    ProjectNameTaken { context_id: i64, name: String },
    #[error("Context {context_id} does not exist")]
    ContextNotFound { context_id: i64 },
    #[error("Project {project_id} does not exist")]
    ProjectNotFound { project_id: i64 },
    #[error("Project {project_id} deletion selection changed; review the deletion preview again")]
    ProjectDeletionPlanMismatch { project_id: i64 },
    #[error("Project {project_id} has active Runs: {run_ids:?}")]
    ProjectHasActiveRuns { project_id: i64, run_ids: Vec<i64> },
    #[error("Context {context_id} deletion selection changed; review the deletion preview again")]
    ContextDeletionPlanMismatch { context_id: i64 },
    #[error("Context {context_id} has active Runs: {run_ids:?}")]
    ContextHasActiveRuns { context_id: i64, run_ids: Vec<i64> },
    #[error("local-data reset has active Runs: {run_ids:?}")]
    ResetHasActiveRuns { run_ids: Vec<i64> },
    #[error("the last Context cannot be deleted")]
    CannotDeleteLastContext,
    #[error("Project {project_id} belongs to another Context")]
    ProjectContextMismatch { project_id: i64, context_id: i64 },
    #[error("a Repository name cannot be blank")]
    EmptyRepositoryName,
    #[error("a Repository name must be a single directory name")]
    InvalidRepositoryName,
    #[error("a Repository remote URL cannot be blank")]
    EmptyRepositoryRemoteUrl,
    #[error("a Repository base branch cannot be blank")]
    EmptyRepositoryBaseBranch,
    #[error("a Repository checkout path cannot be blank")]
    EmptyRepositoryCheckoutPath,
    #[error("a Repository Worktree root cannot be blank")]
    EmptyRepositoryWorktreeRoot,
    #[error("Repository name already exists in Project {project_id}: {name}")]
    RepositoryNameTaken { project_id: i64, name: String },
    #[error("Repository {name} in Project {project_id} has a different remote URL")]
    RepositoryRemoteMismatch { project_id: i64, name: String },
    #[error("Repository {repository_id} does not exist")]
    RepositoryNotFound { repository_id: i64 },
    #[error("Repository {repository_id} belongs to another Project than {project_id}")]
    RepositoryProjectMismatch { repository_id: i64, project_id: i64 },
    #[error("Repository {repository_id} has already been configured on Machine {machine_id}")]
    RepositoryLocationAlreadyExists { repository_id: i64, machine_id: i64 },
    #[error(
        "Repository {repository_id} deletion must include Workspaces {expected_workspace_ids:?}; received {provided_workspace_ids:?}"
    )]
    RepositoryWorkspacesMismatch {
        repository_id: i64,
        expected_workspace_ids: Vec<i64>,
        provided_workspace_ids: Vec<i64>,
    },
    #[error(
        "Machine {machine_id} deletion must include Runs {expected_run_ids:?}; received {provided_run_ids:?}"
    )]
    MachineRunsMismatch {
        machine_id: i64,
        expected_run_ids: Vec<i64>,
        provided_run_ids: Vec<i64>,
    },
    #[error("a branch cannot be blank")]
    EmptyBranch,
    #[error("a Workspace must include at least one Repository")]
    EmptyWorkspaceRepositories,
    #[error("Workspace {workspace_id} does not exist")]
    WorkspaceNotFound { workspace_id: i64 },
    #[error("Workspace {workspace_id} has Run history and cannot be removed")]
    WorkspaceHasRuns { workspace_id: i64 },
    #[error("Workspace {workspace_id} belongs to another Item")]
    WorkspaceItemMismatch { workspace_id: i64, item_id: i64 },
    #[error("Repository {repository_id} is not selected in Workspace {workspace_id}")]
    RunRepositoryNotSelected {
        repository_id: i64,
        workspace_id: i64,
    },
    #[error("Worktree {worktree_id} does not belong to Workspace {workspace_id}")]
    RunWorktreeWorkspaceMismatch { worktree_id: i64, workspace_id: i64 },
    #[error("Run working directory does not match Repository {repository_id}")]
    RunWorkingDirectoryRepositoryMismatch { repository_id: i64 },
    #[error("Repository {repository_id} is already in Workspace {workspace_id}")]
    RepositoryAlreadyInWorkspace {
        repository_id: i64,
        workspace_id: i64,
    },
    #[error("Worktree {worktree_id} does not exist")]
    WorktreeNotFound { worktree_id: i64 },
    #[error("Repository {repository_id} already has a Worktree in Workspace {workspace_id}")]
    WorktreeAlreadyExists {
        repository_id: i64,
        workspace_id: i64,
    },
    #[error("Worktree Repository {repository_id} is not selected in Workspace {workspace_id}")]
    WorktreeRepositoryNotSelected {
        repository_id: i64,
        workspace_id: i64,
    },
    #[error("a Worktree path cannot be blank")]
    EmptyWorktreePath,
    #[error("a Machine name cannot be blank")]
    EmptyMachineName,
    #[error("a Machine socket name cannot be blank")]
    EmptyMachineSocketName,
    #[error("Machine name already exists in Context {context_id}: {name}")]
    MachineNameTaken { context_id: i64, name: String },
    #[error("Machine {machine_id} does not exist")]
    MachineNotFound { machine_id: i64 },
    #[error("Machine {machine_id} belongs to another Context")]
    MachineContextMismatch { machine_id: i64, context_id: i64 },
    #[error("Machine {machine_id} has active Runs: {run_ids:?}")]
    MachineHasActiveRuns { machine_id: i64, run_ids: Vec<i64> },
    #[error("a remote Machine host cannot be blank")]
    EmptyMachineHost,
    #[error("a remote Machine host contains unsupported characters")]
    InvalidMachineHost,
    #[error("a remote Machine user cannot be blank")]
    EmptyMachineUser,
    #[error("a remote Machine user contains unsupported characters")]
    InvalidMachineUser,
    #[error("a remote Machine SSH port must be positive")]
    InvalidMachinePort,
    #[error("a remote Machine host-key checking mode is unsupported")]
    InvalidMachineHostKeyChecking,
    #[error("a Run prompt cannot be blank")]
    EmptyRunPrompt,
    #[error("a Run working directory cannot be blank")]
    EmptyRunWorkingDirectory,
    #[error("Direct Run must include at least one Repository")]
    EmptyDirectRunCheckouts,
    #[error("Direct Run checkout for Repository {repository_id} is invalid")]
    InvalidDirectRunCheckout { repository_id: i64 },
    #[error("Direct Run must choose a primary Repository")]
    MissingDirectRunRepository,
    #[error("Direct Run has dirty checkouts for Repositories {repository_ids:?}; confirm the warning before starting")]
    DirectRunDirtyCheckouts { repository_ids: Vec<i64> },
    #[error("Direct Run shares checkout paths with active Runs {run_ids:?}: {paths:?}; confirm the shared checkout warning before starting")]
    DirectRunSharedCheckouts {
        run_ids: Vec<i64>,
        paths: Vec<String>,
    },
    #[error("a Run session name cannot be blank")]
    EmptyRunSessionName,
    #[error("a Run Pane identity cannot be blank")]
    EmptyRunPaneId,
    #[error("Grill configuration is invalid for {agent:?}: model {model}, effort {effort}")]
    InvalidGrillConfiguration {
        agent: AgentKind,
        model: String,
        effort: String,
    },
    #[error("Item {item_id} already has an active Grill Run {run_id}")]
    ActiveGrillRun { item_id: i64, run_id: i64 },
    #[error("a Grill skill snapshot cannot be blank")]
    EmptyGrillSkillSnapshot,
    #[error("a Grill answer cannot be blank")]
    EmptyGrillAnswer,
    #[error("a Grill question cannot have more than one answer")]
    DuplicateGrillAnswer,
    #[error("Run {run_id} has no parsed Grill question group")]
    GrillQuestionGroupNotFound { run_id: i64 },
    #[error("Grill answer is for unknown question {question_number}")]
    UnknownGrillQuestion { question_number: u32 },
    #[error("Run {run_id} already has a submitted Grill response")]
    GrillResponseAlreadySubmitted { run_id: i64 },
    #[error("Run {run_id} is not a Grill Run")]
    NotGrillRun { run_id: i64 },
    #[error("a Grill response cannot be blank")]
    EmptyGrillResponse,
    #[error("Run {run_id} does not exist")]
    RunNotFound { run_id: i64 },
    #[error("Run {run_id} cannot continue Grill from phase {phase:?}")]
    GrillContinuationNotAvailable {
        run_id: i64,
        phase: Option<GrillPhase>,
    },
    #[error("Run {run_id} cannot capture downstream Issues for action {action:?}")]
    DownstreamCaptureNotAvailable {
        run_id: i64,
        action: GrillContinuationAction,
    },
    #[error("Run {run_id} is {state:?}; stop it and wait for Finished state before deleting")]
    RunNotFinished { run_id: i64, state: RunState },
    #[error(
        "a Run is already attached to Machine {machine_id}, session {session_name}, Pane {pane_id}"
    )]
    RunAlreadyAttached {
        machine_id: i64,
        session_name: String,
        pane_id: String,
    },
    #[error("External Object {external_object_id} is not linked to Item {item_id}")]
    RunPromptSourceNotLinked {
        external_object_id: i64,
        item_id: i64,
    },
    #[error("Repository {repository_id} was selected more than once")]
    DuplicateRepositorySelection { repository_id: i64 },
    #[error("Item {item_id} does not exist")]
    ItemNotFound { item_id: i64 },
    #[error("Item {item_id} has active Runs: {run_ids:?}")]
    ItemHasActiveRuns { item_id: i64, run_ids: Vec<i64> },
    #[error("Items {from_item_id} and {to_item_id} belong to different Contexts")]
    ItemContextMismatch { from_item_id: i64, to_item_id: i64 },
    #[error("an Item cannot relate to itself: {item_id}")]
    SelfRelation { item_id: i64 },
    #[error("the relationship already exists")]
    RelationAlreadyExists,
    #[error("the Item identifier sequence is exhausted")]
    SequenceExhausted,
    #[error("an external URL cannot be blank")]
    EmptyExternalUrl,
    #[error("an external object key cannot be blank")]
    EmptyExternalObjectKey,
    #[error("External Object {external_object_id} does not exist")]
    ExternalObjectNotFound { external_object_id: i64 },
    #[error("the Link already exists")]
    LinkAlreadyExists,
    #[error("Link {link_id} does not exist")]
    LinkNotFound { link_id: i64 },
    #[error("Reminder {reminder_id} does not exist on Item {item_id}")]
    ReminderNotFound { item_id: i64, reminder_id: i64 },
}

/// Stores machine paths in a stable form while keeping paths under the machine home readable.
pub fn normalize_machine_path(input: &str, machine_home: &str) -> Result<String, DomainError> {
    let input = input.trim().trim_end_matches('/');
    if input.is_empty() {
        return Err(DomainError::EmptyRepositoryCheckoutPath);
    }
    if input == "~" || input.starts_with("~/") {
        return Ok(input.to_owned());
    }

    let home = machine_home.trim().trim_end_matches('/');
    if !home.is_empty() && (input == home || input.starts_with(&format!("{home}/"))) {
        let relative = input[home.len()..].trim_start_matches('/');
        return Ok(if relative.is_empty() {
            "~".into()
        } else {
            format!("~/{relative}")
        });
    }

    if input.starts_with('/') {
        Ok(input.to_owned())
    } else {
        Ok(format!("~/{input}"))
    }
}

pub fn decide(mut state: DomainState, event: Event) -> Result<Decision, DomainError> {
    match event {
        Event::CreateContext { name } => {
            let name = clean_name(name, DomainError::EmptyContextName)?;
            if state.contexts.iter().any(|context| context.name == name) {
                return Err(DomainError::ContextNameTaken { name });
            }

            let id = state.next_context_id;
            let next_context_id = id.checked_add(1).ok_or(DomainError::SequenceExhausted)?;
            let project_id = state.next_project_id;
            let next_project_id = project_id
                .checked_add(1)
                .ok_or(DomainError::SequenceExhausted)?;
            let context = Context {
                id,
                name,
                grill_defaults: GrillConfiguration::default(),
            };
            let project = Project {
                id: project_id,
                context_id: id,
                name: "Default".into(),
                defaults: ProjectDefaults::default(),
            };

            state.next_context_id = next_context_id;
            state.next_project_id = next_project_id;
            state.contexts.push(context.clone());
            state.projects.push(project.clone());

            Ok(Decision {
                state,
                effects: vec![
                    Effect::PersistContext {
                        context,
                        next_context_id,
                    },
                    Effect::PersistProject {
                        project,
                        next_project_id,
                    },
                ],
            })
        }
        Event::SetContextGrillDefaults {
            context_id,
            defaults,
        } => {
            validate_grill_configuration(&defaults)?;
            let context = state
                .contexts
                .iter_mut()
                .find(|context| context.id == context_id)
                .ok_or(DomainError::ContextNotFound { context_id })?;
            context.grill_defaults = defaults;
            let context = context.clone();
            Ok(Decision {
                state,
                effects: vec![Effect::PersistContextGrillDefaults { context }],
            })
        }
        Event::CreateProject {
            context_id,
            name,
            defaults,
        } => {
            let name = clean_name(name, DomainError::EmptyProjectName)?;
            ensure_context(&state, context_id)?;
            if state
                .projects
                .iter()
                .any(|project| project.context_id == context_id && project.name == name)
            {
                return Err(DomainError::ProjectNameTaken { context_id, name });
            }

            let id = state.next_project_id;
            let next_project_id = id.checked_add(1).ok_or(DomainError::SequenceExhausted)?;
            let project = Project {
                id,
                context_id,
                name,
                defaults,
            };

            state.next_project_id = next_project_id;
            state.projects.push(project.clone());

            Ok(Decision {
                state,
                effects: vec![Effect::PersistProject {
                    project,
                    next_project_id,
                }],
            })
        }
        Event::RegisterRepository {
            project_id,
            name,
            remote_url,
        } => {
            let name = clean_name(name, DomainError::EmptyRepositoryName)?;
            if name == "." || name == ".." || name.contains('/') || name.contains('\\') {
                return Err(DomainError::InvalidRepositoryName);
            }
            let remote_url = clean_name(remote_url, DomainError::EmptyRepositoryRemoteUrl)?;
            let project = state
                .projects
                .iter()
                .find(|project| project.id == project_id)
                .ok_or(DomainError::ProjectNotFound { project_id })?;
            if state
                .repositories
                .iter()
                .any(|repository| repository.project_id == project.id && repository.name == name)
            {
                return Err(DomainError::RepositoryNameTaken { project_id, name });
            }

            let id = state.next_repository_id;
            let next_repository_id = id.checked_add(1).ok_or(DomainError::SequenceExhausted)?;
            let repository = Repository {
                id,
                project_id,
                name,
                remote_url,
                base_branch: "main".into(),
            };
            state.next_repository_id = next_repository_id;
            state.repositories.push(repository.clone());

            Ok(Decision {
                state,
                effects: vec![Effect::PersistRepository {
                    repository,
                    next_repository_id,
                }],
            })
        }
        Event::RegisterRepositoryAtLocation {
            project_id,
            name,
            remote_url,
            base_branch,
            machine_id,
            checkout_path,
            worktree_root,
        } => {
            let name = clean_repository_name(name)?;
            let remote_url = clean_name(remote_url, DomainError::EmptyRepositoryRemoteUrl)?;
            let base_branch = clean_name(base_branch, DomainError::EmptyRepositoryBaseBranch)?;
            let project = state
                .projects
                .iter()
                .find(|project| project.id == project_id)
                .ok_or(DomainError::ProjectNotFound { project_id })?;
            let machine = state
                .machines
                .iter()
                .find(|machine| machine.id == machine_id)
                .ok_or(DomainError::MachineNotFound { machine_id })?;
            if machine.context_id != project.context_id {
                return Err(DomainError::MachineContextMismatch {
                    machine_id,
                    context_id: project.context_id,
                });
            }
            let checkout_path =
                clean_name(checkout_path, DomainError::EmptyRepositoryCheckoutPath)?;
            let worktree_root =
                clean_name(worktree_root, DomainError::EmptyRepositoryWorktreeRoot)?;
            let location = RepositoryLocation {
                repository_id: 0,
                machine_id,
                checkout_path,
                worktree_root,
            };

            if let Some(repository) = state
                .repositories
                .iter_mut()
                .find(|repository| repository.project_id == project_id && repository.name == name)
            {
                if repository.remote_url != remote_url {
                    return Err(DomainError::RepositoryRemoteMismatch { project_id, name });
                }
                if state.repository_locations.iter().any(|candidate| {
                    candidate.repository_id == repository.id && candidate.machine_id == machine_id
                }) {
                    return Err(DomainError::RepositoryLocationAlreadyExists {
                        repository_id: repository.id,
                        machine_id,
                    });
                }
                repository.base_branch = base_branch;
                let repository = repository.clone();
                let location = RepositoryLocation {
                    repository_id: repository.id,
                    ..location
                };
                state.repository_locations.push(location.clone());
                return Ok(Decision {
                    state,
                    effects: vec![
                        Effect::UpdateRepository { repository },
                        Effect::PersistRepositoryLocation { location },
                    ],
                });
            }

            let id = state.next_repository_id;
            let next_repository_id = id.checked_add(1).ok_or(DomainError::SequenceExhausted)?;
            let repository = Repository {
                id,
                project_id,
                name,
                remote_url,
                base_branch,
            };
            let location = RepositoryLocation {
                repository_id: id,
                ..location
            };
            state.next_repository_id = next_repository_id;
            state.repositories.push(repository.clone());
            state.repository_locations.push(location.clone());

            Ok(Decision {
                state,
                effects: vec![
                    Effect::PersistRepository {
                        repository,
                        next_repository_id,
                    },
                    Effect::PersistRepositoryLocation { location },
                ],
            })
        }
        Event::ConfigureRepositoryLocation {
            repository_id,
            machine_id,
            checkout_path,
            worktree_root,
        } => {
            let repository = state
                .repositories
                .iter()
                .find(|repository| repository.id == repository_id)
                .ok_or(DomainError::RepositoryNotFound { repository_id })?;
            let project = state
                .projects
                .iter()
                .find(|project| project.id == repository.project_id)
                .ok_or(DomainError::ProjectNotFound {
                    project_id: repository.project_id,
                })?;
            let machine = state
                .machines
                .iter()
                .find(|machine| machine.id == machine_id)
                .ok_or(DomainError::MachineNotFound { machine_id })?;
            if machine.context_id != project.context_id {
                return Err(DomainError::MachineContextMismatch {
                    machine_id,
                    context_id: project.context_id,
                });
            }
            if state.repository_locations.iter().any(|candidate| {
                candidate.repository_id == repository_id && candidate.machine_id == machine_id
            }) {
                return Err(DomainError::RepositoryLocationAlreadyExists {
                    repository_id,
                    machine_id,
                });
            }
            let location = RepositoryLocation {
                repository_id,
                machine_id,
                checkout_path: clean_name(checkout_path, DomainError::EmptyRepositoryCheckoutPath)?,
                worktree_root: clean_name(worktree_root, DomainError::EmptyRepositoryWorktreeRoot)?,
            };
            state.repository_locations.push(location.clone());
            Ok(Decision {
                state,
                effects: vec![Effect::PersistRepositoryLocation { location }],
            })
        }
        Event::ResetLocalData => {
            let active_run_ids = state
                .runs
                .iter()
                .filter(|run| run_is_active(run))
                .map(|run| run.id)
                .collect::<Vec<_>>();
            if !active_run_ids.is_empty() {
                return Err(DomainError::ResetHasActiveRuns {
                    run_ids: active_run_ids,
                });
            }
            let context_id = state.next_context_id;
            let next_context_id = context_id
                .checked_add(1)
                .ok_or(DomainError::SequenceExhausted)?;
            let project_id = state.next_project_id;
            let next_project_id = project_id
                .checked_add(1)
                .ok_or(DomainError::SequenceExhausted)?;
            let context = Context {
                id: context_id,
                name: "Personal".into(),
                grill_defaults: GrillConfiguration::default(),
            };
            let project = Project {
                id: project_id,
                context_id,
                name: "Default".into(),
                defaults: ProjectDefaults::default(),
            };

            state.next_context_id = next_context_id;
            state.next_project_id = next_project_id;
            state.contexts = vec![context.clone()];
            state.projects = vec![project.clone()];
            state.repositories.clear();
            state.repository_locations.clear();
            state.items.clear();
            state.workspaces.clear();
            state.worktrees.clear();
            state.machines.clear();
            state.runs.clear();
            state.relationships.clear();
            state.external_objects.clear();
            state.links.clear();
            state.snapshots.clear();
            state.activities.clear();
            state.attention_defaults.clear();

            Ok(Decision {
                state,
                effects: vec![Effect::ResetLocalData {
                    context,
                    project,
                    next_context_id,
                    next_project_id,
                }],
            })
        }
        Event::DeleteRepository {
            repository_id,
            workspace_ids,
        } => {
            let plan = plan_repository_deletion(&state, repository_id)?;
            let mut expected_workspace_ids = plan
                .workspaces
                .iter()
                .map(|workspace| workspace.id)
                .collect::<Vec<_>>();
            let mut provided_workspace_ids = workspace_ids;
            expected_workspace_ids.sort_unstable();
            provided_workspace_ids.sort_unstable();
            if expected_workspace_ids != provided_workspace_ids {
                return Err(DomainError::RepositoryWorkspacesMismatch {
                    repository_id,
                    expected_workspace_ids,
                    provided_workspace_ids,
                });
            }
            if let Some(workspace_id) = plan.workspaces.iter().find_map(|workspace| {
                state
                    .runs
                    .iter()
                    .any(|run| run.workspace_id == Some(workspace.id))
                    .then_some(workspace.id)
            }) {
                return Err(DomainError::WorkspaceHasRuns { workspace_id });
            }

            for workspace in &plan.workspaces {
                state
                    .worktrees
                    .retain(|worktree| worktree.workspace_id != workspace.id);
                state
                    .workspaces
                    .retain(|candidate| candidate.id != workspace.id);
            }
            state
                .repositories
                .retain(|repository| repository.id != repository_id);
            state
                .repository_locations
                .retain(|location| location.repository_id != repository_id);

            let mut effects = plan
                .workspaces
                .iter()
                .map(|workspace| Effect::RemoveWorkspace {
                    workspace_id: workspace.id,
                })
                .collect::<Vec<_>>();
            effects.push(Effect::RemoveRepository { repository_id });

            Ok(Decision { state, effects })
        }
        Event::DeleteProject {
            project_id,
            item_ids,
            repository_ids,
            workspace_ids,
        } => {
            let plan = plan_project_deletion(&state, project_id)?;
            if !parent_selection_matches(
                &plan.items.iter().map(|item| item.id).collect::<Vec<_>>(),
                item_ids,
            ) || !parent_selection_matches(
                &plan
                    .repositories
                    .iter()
                    .map(|repository| repository.id)
                    .collect::<Vec<_>>(),
                repository_ids,
            ) || !parent_selection_matches(
                &plan
                    .workspaces
                    .iter()
                    .map(|workspace| workspace.id)
                    .collect::<Vec<_>>(),
                workspace_ids,
            ) {
                return Err(DomainError::ProjectDeletionPlanMismatch { project_id });
            }
            if !plan.active_run_ids.is_empty() {
                return Err(DomainError::ProjectHasActiveRuns {
                    project_id,
                    run_ids: plan.active_run_ids.clone(),
                });
            }

            let summary = plan.summary();
            let orphaned_external_object_ids = plan.orphaned_external_object_ids.clone();
            let item_ids = plan.items.iter().map(|item| item.id).collect::<Vec<_>>();
            state.items.retain(|item| !item_ids.contains(&item.id));
            state.worktrees.retain(|worktree| {
                !plan
                    .workspaces
                    .iter()
                    .any(|workspace| workspace.id == worktree.workspace_id)
            });
            state.workspaces.retain(|workspace| {
                !plan
                    .workspaces
                    .iter()
                    .any(|candidate| candidate.id == workspace.id)
            });
            state
                .runs
                .retain(|run| !plan.runs.iter().any(|candidate| candidate.id == run.id));
            state.relationships.retain(|relation| {
                !item_ids.contains(&relation.from_item_id)
                    && !item_ids.contains(&relation.to_item_id)
            });
            state.links.retain(|link| !item_ids.contains(&link.item_id));
            state
                .external_objects
                .retain(|object| !plan.orphaned_external_object_ids.contains(&object.id));
            state.snapshots.retain(|snapshot| {
                !plan
                    .orphaned_external_object_ids
                    .contains(&snapshot.external_object_id)
            });
            state.activities.retain(|activity| {
                !plan
                    .orphaned_external_object_ids
                    .contains(&activity.external_object_id)
            });
            state.repositories.retain(|repository| {
                !plan
                    .repositories
                    .iter()
                    .any(|candidate| candidate.id == repository.id)
            });
            let repository_ids = plan
                .repositories
                .iter()
                .map(|repository| repository.id)
                .collect::<Vec<_>>();
            state
                .repository_locations
                .retain(|location| !repository_ids.contains(&location.repository_id));
            state.projects.retain(|project| project.id != project_id);

            Ok(Decision {
                state,
                effects: vec![Effect::RemoveProjectCascade {
                    project_id,
                    item_ids,
                    repository_ids: plan
                        .repositories
                        .iter()
                        .map(|repository| repository.id)
                        .collect(),
                    workspace_ids: plan
                        .workspaces
                        .iter()
                        .map(|workspace| workspace.id)
                        .collect(),
                    orphaned_external_object_ids,
                    summary,
                }],
            })
        }
        Event::DeleteContext {
            context_id,
            project_ids,
            item_ids,
            repository_ids,
            workspace_ids,
            machine_ids,
        } => {
            let plan = plan_context_deletion(&state, context_id)?;
            if state.contexts.len() == 1 {
                return Err(DomainError::CannotDeleteLastContext);
            }
            if !parent_selection_matches(
                &plan
                    .projects
                    .iter()
                    .map(|project| project.id)
                    .collect::<Vec<_>>(),
                project_ids,
            ) || !parent_selection_matches(
                &plan.items.iter().map(|item| item.id).collect::<Vec<_>>(),
                item_ids,
            ) || !parent_selection_matches(
                &plan
                    .repositories
                    .iter()
                    .map(|repository| repository.id)
                    .collect::<Vec<_>>(),
                repository_ids,
            ) || !parent_selection_matches(
                &plan
                    .workspaces
                    .iter()
                    .map(|workspace| workspace.id)
                    .collect::<Vec<_>>(),
                workspace_ids,
            ) || !parent_selection_matches(
                &plan
                    .machines
                    .iter()
                    .map(|machine| machine.id)
                    .collect::<Vec<_>>(),
                machine_ids,
            ) {
                return Err(DomainError::ContextDeletionPlanMismatch { context_id });
            }
            if !plan.active_run_ids.is_empty() {
                return Err(DomainError::ContextHasActiveRuns {
                    context_id,
                    run_ids: plan.active_run_ids.clone(),
                });
            }

            let summary = plan.summary();
            let orphaned_external_object_ids = plan.orphaned_external_object_ids.clone();
            let item_ids = plan.items.iter().map(|item| item.id).collect::<Vec<_>>();
            let project_ids = plan
                .projects
                .iter()
                .map(|project| project.id)
                .collect::<Vec<_>>();
            let machine_ids = plan
                .machines
                .iter()
                .map(|machine| machine.id)
                .collect::<Vec<_>>();
            state.contexts.retain(|context| context.id != context_id);
            state
                .projects
                .retain(|project| !project_ids.contains(&project.id));
            state.items.retain(|item| !item_ids.contains(&item.id));
            state.repositories.retain(|repository| {
                !plan
                    .repositories
                    .iter()
                    .any(|candidate| candidate.id == repository.id)
            });
            let repository_ids = plan
                .repositories
                .iter()
                .map(|repository| repository.id)
                .collect::<Vec<_>>();
            state
                .repository_locations
                .retain(|location| !repository_ids.contains(&location.repository_id));
            state.worktrees.retain(|worktree| {
                !plan
                    .workspaces
                    .iter()
                    .any(|workspace| workspace.id == worktree.workspace_id)
            });
            state.workspaces.retain(|workspace| {
                !plan
                    .workspaces
                    .iter()
                    .any(|candidate| candidate.id == workspace.id)
            });
            state
                .runs
                .retain(|run| !plan.runs.iter().any(|candidate| candidate.id == run.id));
            state
                .machines
                .retain(|machine| !machine_ids.contains(&machine.id));
            state.relationships.retain(|relation| {
                !item_ids.contains(&relation.from_item_id)
                    && !item_ids.contains(&relation.to_item_id)
            });
            state.links.retain(|link| !item_ids.contains(&link.item_id));
            state
                .external_objects
                .retain(|object| !plan.orphaned_external_object_ids.contains(&object.id));
            state.snapshots.retain(|snapshot| {
                !plan
                    .orphaned_external_object_ids
                    .contains(&snapshot.external_object_id)
            });
            state.activities.retain(|activity| {
                !plan
                    .orphaned_external_object_ids
                    .contains(&activity.external_object_id)
            });
            state
                .attention_defaults
                .retain(|attention_default| attention_default.context_id != context_id);

            Ok(Decision {
                state,
                effects: vec![Effect::RemoveContextCascade {
                    context_id,
                    project_ids,
                    item_ids,
                    repository_ids: plan
                        .repositories
                        .iter()
                        .map(|repository| repository.id)
                        .collect(),
                    workspace_ids: plan
                        .workspaces
                        .iter()
                        .map(|workspace| workspace.id)
                        .collect(),
                    machine_ids,
                    orphaned_external_object_ids,
                    summary,
                }],
            })
        }
        Event::DeleteMachine {
            machine_id,
            run_ids,
        } => {
            let plan = plan_machine_deletion(&state, machine_id)?;
            let mut expected_run_ids = plan.runs.iter().map(|run| run.id).collect::<Vec<_>>();
            let mut provided_run_ids = run_ids;
            expected_run_ids.sort_unstable();
            provided_run_ids.sort_unstable();
            if expected_run_ids != provided_run_ids {
                return Err(DomainError::MachineRunsMismatch {
                    machine_id,
                    expected_run_ids,
                    provided_run_ids,
                });
            }
            if !plan.active_run_ids.is_empty() {
                return Err(DomainError::MachineHasActiveRuns {
                    machine_id,
                    run_ids: plan.active_run_ids,
                });
            }

            state.runs.retain(|run| run.machine_id != machine_id);
            state.machines.retain(|machine| machine.id != machine_id);
            state
                .repository_locations
                .retain(|location| location.machine_id != machine_id);
            let mut effects = plan
                .runs
                .iter()
                .map(|run| Effect::RemoveRun { run_id: run.id })
                .collect::<Vec<_>>();
            effects.push(Effect::RemoveMachine { machine_id });

            Ok(Decision { state, effects })
        }
        Event::CreateItem {
            title,
            context_id,
            project_id,
        } => {
            if title.trim().is_empty() {
                return Err(DomainError::EmptyTitle);
            }
            ensure_context(&state, context_id)?;
            let project = state
                .projects
                .iter()
                .find(|project| project.id == project_id)
                .ok_or(DomainError::ProjectNotFound { project_id })?;
            if project.context_id != context_id {
                return Err(DomainError::ProjectContextMismatch {
                    project_id,
                    context_id,
                });
            }

            let id = state.next_item_id;
            let number = state.next_item_number;
            let next_item_id = id.checked_add(1).ok_or(DomainError::SequenceExhausted)?;
            let next_item_number = number
                .checked_add(1)
                .ok_or(DomainError::SequenceExhausted)?;
            let item = Item {
                id,
                human_identifier: format!("MC-{number}"),
                title: title.trim().to_owned(),
                project_id,
                status: project.defaults.item_status,
                notes: String::new(),
                reminders: Vec::new(),
            };

            state.next_item_id = next_item_id;
            state.next_item_number = next_item_number;
            state.items.push(item.clone());

            Ok(Decision {
                state,
                effects: vec![Effect::PersistItem {
                    item,
                    next_item_number,
                    next_item_id,
                }],
            })
        }
        Event::CreateWorkspace {
            item_id,
            repositories,
        } => {
            let project_id = item_project_id(&state, item_id)?;
            if repositories.is_empty() {
                return Err(DomainError::EmptyWorkspaceRepositories);
            }
            let repositories = normalize_workspace_repositories(&state, project_id, repositories)?;
            let id = state.next_workspace_id;
            let next_workspace_id = id.checked_add(1).ok_or(DomainError::SequenceExhausted)?;
            let workspace = Workspace {
                id,
                item_id,
                repositories,
                preparation_state: WorkspacePreparationState::Pending,
            };
            state.next_workspace_id = next_workspace_id;
            state.workspaces.push(workspace.clone());

            Ok(Decision {
                state,
                effects: vec![Effect::PersistWorkspace {
                    workspace,
                    next_workspace_id,
                }],
            })
        }
        Event::CreateWorktree {
            workspace_id,
            repository_id,
            machine_id,
            path,
            branch,
            base_branch,
            is_dirty,
        } => {
            let workspace = state
                .workspaces
                .iter()
                .find(|workspace| workspace.id == workspace_id)
                .ok_or(DomainError::WorkspaceNotFound { workspace_id })?;
            if !workspace
                .repositories
                .iter()
                .any(|repository| repository.repository_id == repository_id)
            {
                return Err(DomainError::WorktreeRepositoryNotSelected {
                    repository_id,
                    workspace_id,
                });
            }
            if state.worktrees.iter().any(|worktree| {
                worktree.workspace_id == workspace_id && worktree.repository_id == repository_id
            }) {
                return Err(DomainError::WorktreeAlreadyExists {
                    repository_id,
                    workspace_id,
                });
            }
            let item_context_id = item_context_id(&state, workspace.item_id)?;
            let machine = state
                .machines
                .iter()
                .find(|machine| machine.id == machine_id)
                .ok_or(DomainError::MachineNotFound { machine_id })?;
            if machine.context_id != item_context_id {
                return Err(DomainError::MachineContextMismatch {
                    machine_id,
                    context_id: item_context_id,
                });
            }
            let path = clean_name(path, DomainError::EmptyWorktreePath)?;
            let branch = clean_name(branch, DomainError::EmptyBranch)?;
            let base_branch = clean_name(base_branch, DomainError::EmptyBranch)?;
            let id = state.next_worktree_id;
            let next_worktree_id = id.checked_add(1).ok_or(DomainError::SequenceExhausted)?;
            let worktree = Worktree {
                id,
                workspace_id,
                repository_id,
                machine_id,
                path,
                branch,
                base_branch,
                is_dirty,
            };
            state.next_worktree_id = next_worktree_id;
            state.worktrees.push(worktree.clone());

            let preparation_state =
                workspace_preparation_state(&state, workspace_id, workspace.preparation_state);
            let workspace = state
                .workspaces
                .iter_mut()
                .find(|workspace| workspace.id == workspace_id)
                .expect("the Workspace was checked above");
            let workspace_changed = workspace.preparation_state != preparation_state;
            workspace.preparation_state = preparation_state;
            let workspace = workspace.clone();
            let mut effects = vec![Effect::PersistWorktree {
                worktree,
                next_worktree_id,
            }];
            if workspace_changed {
                effects.push(Effect::PersistWorkspaceUpdate { workspace });
            }

            Ok(Decision { state, effects })
        }
        Event::MarkWorkspaceResumable { workspace_id } => {
            let workspace = state
                .workspaces
                .iter_mut()
                .find(|workspace| workspace.id == workspace_id)
                .ok_or(DomainError::WorkspaceNotFound { workspace_id })?;
            workspace.preparation_state = WorkspacePreparationState::Resumable;
            let workspace = workspace.clone();

            Ok(Decision {
                state,
                effects: vec![Effect::PersistWorkspaceUpdate { workspace }],
            })
        }
        Event::RemoveWorktree { worktree_id } => {
            let position = state
                .worktrees
                .iter()
                .position(|worktree| worktree.id == worktree_id)
                .ok_or(DomainError::WorktreeNotFound { worktree_id })?;
            let workspace_id = state.worktrees[position].workspace_id;
            state.worktrees.remove(position);
            let previous_workspace = state
                .workspaces
                .iter()
                .find(|workspace| workspace.id == workspace_id)
                .cloned()
                .ok_or(DomainError::WorkspaceNotFound { workspace_id })?;
            let preparation_state = workspace_preparation_state(
                &state,
                workspace_id,
                previous_workspace.preparation_state,
            );
            let workspace = state
                .workspaces
                .iter_mut()
                .find(|workspace| workspace.id == workspace_id)
                .expect("the Workspace was checked above");
            workspace.preparation_state = preparation_state;
            let workspace = workspace.clone();

            Ok(Decision {
                state,
                effects: vec![
                    Effect::RemoveWorktree { worktree_id },
                    Effect::PersistWorkspaceUpdate { workspace },
                ],
            })
        }
        Event::RemoveWorkspace { workspace_id } => {
            if !state
                .workspaces
                .iter()
                .any(|workspace| workspace.id == workspace_id)
            {
                return Err(DomainError::WorkspaceNotFound { workspace_id });
            }
            if state
                .runs
                .iter()
                .any(|run| run.workspace_id == Some(workspace_id))
            {
                return Err(DomainError::WorkspaceHasRuns { workspace_id });
            }
            state
                .worktrees
                .retain(|worktree| worktree.workspace_id != workspace_id);
            state
                .workspaces
                .retain(|workspace| workspace.id != workspace_id);

            Ok(Decision {
                state,
                effects: vec![Effect::RemoveWorkspace { workspace_id }],
            })
        }
        Event::RegisterMachine {
            context_id,
            name,
            socket_name,
            transport,
        } => {
            ensure_context(&state, context_id)?;
            let name = clean_name(name, DomainError::EmptyMachineName)?;
            let socket_name = clean_name(socket_name, DomainError::EmptyMachineSocketName)?;
            let transport = clean_machine_transport(transport)?;
            if state
                .machines
                .iter()
                .any(|machine| machine.context_id == context_id && machine.name == name)
            {
                return Err(DomainError::MachineNameTaken { context_id, name });
            }
            let id = state.next_machine_id;
            let next_machine_id = id.checked_add(1).ok_or(DomainError::SequenceExhausted)?;
            let machine = Machine {
                id,
                context_id,
                name,
                socket_name,
                transport,
                last_observed: MachineObservation::Unknown,
                last_observed_at: None,
            };
            state.next_machine_id = next_machine_id;
            state.machines.push(machine.clone());
            Ok(Decision {
                state,
                effects: vec![Effect::PersistMachine {
                    machine,
                    next_machine_id,
                }],
            })
        }
        Event::ObserveMachine {
            machine_id,
            observation,
            observed_at,
        } => {
            let machine = state
                .machines
                .iter_mut()
                .find(|machine| machine.id == machine_id)
                .ok_or(DomainError::MachineNotFound { machine_id })?;
            machine.last_observed = observation;
            machine.last_observed_at = Some(observed_at);
            let machine = machine.clone();
            Ok(Decision {
                state,
                effects: vec![Effect::PersistMachineObservation { machine }],
            })
        }
        Event::DeleteItem { item_id } => {
            let plan = plan_item_deletion(&state, item_id)?;
            if !plan.active_run_ids.is_empty() {
                return Err(DomainError::ItemHasActiveRuns {
                    item_id,
                    run_ids: plan.active_run_ids,
                });
            }
            let summary = plan.summary();
            let orphaned_external_object_ids = plan.orphaned_external_object_ids.clone();
            state.items.retain(|item| item.id != item_id);
            state.worktrees.retain(|worktree| {
                !state.workspaces.iter().any(|workspace| {
                    workspace.id == worktree.workspace_id && workspace.item_id == item_id
                })
            });
            state
                .workspaces
                .retain(|workspace| workspace.item_id != item_id);
            state.runs.retain(|run| run.item_id != item_id);
            state.relationships.retain(|relation| {
                relation.from_item_id != item_id && relation.to_item_id != item_id
            });
            state.links.retain(|link| link.item_id != item_id);
            state
                .external_objects
                .retain(|object| !orphaned_external_object_ids.contains(&object.id));
            state.snapshots.retain(|snapshot| {
                !orphaned_external_object_ids.contains(&snapshot.external_object_id)
            });
            state.activities.retain(|activity| {
                !orphaned_external_object_ids.contains(&activity.external_object_id)
            });

            Ok(Decision {
                state,
                effects: vec![Effect::RemoveItemCascade {
                    item_id,
                    orphaned_external_object_ids,
                    summary,
                }],
            })
        }
        Event::DeleteLink { link_id } => {
            let link = state
                .links
                .iter()
                .find(|link| link.id == link_id)
                .cloned()
                .ok_or(DomainError::LinkNotFound { link_id })?;
            let external_object_id = link.external_object_id;
            state.links.retain(|candidate| candidate.id != link_id);
            let orphaned = !state
                .links
                .iter()
                .any(|candidate| candidate.external_object_id == external_object_id);
            if orphaned {
                state
                    .external_objects
                    .retain(|object| object.id != external_object_id);
                state
                    .snapshots
                    .retain(|snapshot| snapshot.external_object_id != external_object_id);
                state
                    .activities
                    .retain(|activity| activity.external_object_id != external_object_id);
            }

            let mut effects = vec![Effect::RemoveLink {
                link_id,
                external_object_id,
            }];
            if orphaned {
                effects.push(Effect::RemoveExternalObject { external_object_id });
            }
            Ok(Decision { state, effects })
        }
        Event::DeleteExternalObject { external_object_id } => {
            plan_external_object_deletion(&state, external_object_id)?;
            state
                .links
                .retain(|link| link.external_object_id != external_object_id);
            state
                .external_objects
                .retain(|object| object.id != external_object_id);
            state
                .snapshots
                .retain(|snapshot| snapshot.external_object_id != external_object_id);
            state
                .activities
                .retain(|activity| activity.external_object_id != external_object_id);

            Ok(Decision {
                state,
                effects: vec![Effect::RemoveExternalObject { external_object_id }],
            })
        }
        Event::DeleteRun { run_id } => {
            let position = state
                .runs
                .iter()
                .position(|run| run.id == run_id)
                .ok_or(DomainError::RunNotFound { run_id })?;
            let run_state = state.runs[position].state;
            if !run_is_finished(&state.runs[position]) {
                return Err(DomainError::RunNotFinished {
                    run_id,
                    state: run_state,
                });
            }
            state.runs.remove(position);

            Ok(Decision {
                state,
                effects: vec![Effect::RemoveRun { run_id }],
            })
        }
        Event::StartDirectRun {
            item_id,
            workspace_id,
            machine_id,
            agent,
            execution_profile,
            prompt,
            working_directory,
            session_name,
            pane_id,
            started_at,
            prompt_selection,
            checkouts,
            repository_id,
            allow_dirty,
            allow_shared_checkouts,
        } => {
            let context_id = item_context_id(&state, item_id)?;
            let workspace = state
                .workspaces
                .iter()
                .find(|workspace| workspace.id == workspace_id)
                .ok_or(DomainError::WorkspaceNotFound { workspace_id })?;
            if workspace.item_id != item_id {
                return Err(DomainError::WorkspaceItemMismatch {
                    workspace_id,
                    item_id,
                });
            }
            let machine = state
                .machines
                .iter()
                .find(|machine| machine.id == machine_id)
                .ok_or(DomainError::MachineNotFound { machine_id })?;
            if machine.context_id != context_id {
                return Err(DomainError::MachineContextMismatch {
                    machine_id,
                    context_id,
                });
            }
            if checkouts.is_empty() {
                return Err(DomainError::EmptyDirectRunCheckouts);
            }
            if !workspace
                .repositories
                .iter()
                .any(|selected| selected.repository_id == repository_id)
            {
                return Err(DomainError::RunRepositoryNotSelected {
                    repository_id,
                    workspace_id,
                });
            }
            let selected_repository_ids = workspace
                .repositories
                .iter()
                .map(|repository| repository.repository_id)
                .collect::<Vec<_>>();
            let mut checkout_repository_ids = Vec::new();
            for checkout in &checkouts {
                if checkout.path.trim().is_empty()
                    || checkout.branch.trim().is_empty()
                    || !selected_repository_ids.contains(&checkout.repository_id)
                    || checkout_repository_ids.contains(&checkout.repository_id)
                {
                    return Err(DomainError::InvalidDirectRunCheckout {
                        repository_id: checkout.repository_id,
                    });
                }
                checkout_repository_ids.push(checkout.repository_id);
            }
            if checkout_repository_ids.len() != selected_repository_ids.len() {
                return Err(DomainError::InvalidDirectRunCheckout {
                    repository_id: selected_repository_ids
                        .into_iter()
                        .find(|repository_id| !checkout_repository_ids.contains(repository_id))
                        .unwrap_or_default(),
                });
            }
            for external_object_id in prompt_selection.external_object_ids {
                if !state.links.iter().any(|link| {
                    link.item_id == item_id && link.external_object_id == external_object_id
                }) {
                    return Err(DomainError::RunPromptSourceNotLinked {
                        external_object_id,
                        item_id,
                    });
                }
            }
            let dirty_repository_ids = checkouts
                .iter()
                .filter(|checkout| checkout.is_dirty)
                .map(|checkout| checkout.repository_id)
                .collect::<Vec<_>>();
            if !allow_dirty && !dirty_repository_ids.is_empty() {
                return Err(DomainError::DirectRunDirtyCheckouts {
                    repository_ids: dirty_repository_ids,
                });
            }
            let mut shared_run_ids = Vec::new();
            let mut shared_paths = Vec::new();
            for active_run in state.runs.iter().filter(|run| {
                run.machine_id == machine_id
                    && run_is_active(run)
                    && run.pane_status != RunPaneStatus::Missing
            }) {
                for checkout in &checkouts {
                    if active_run
                        .direct_checkouts
                        .iter()
                        .any(|active_checkout| active_checkout.path == checkout.path)
                    {
                        shared_run_ids.push(active_run.id);
                        shared_paths.push(checkout.path.clone());
                    }
                }
            }
            shared_run_ids.sort_unstable();
            shared_run_ids.dedup();
            shared_paths.sort();
            shared_paths.dedup();
            if !allow_shared_checkouts && !shared_run_ids.is_empty() {
                return Err(DomainError::DirectRunSharedCheckouts {
                    run_ids: shared_run_ids,
                    paths: shared_paths,
                });
            }
            let prompt = clean_name(prompt, DomainError::EmptyRunPrompt)?;
            let working_directory =
                clean_name(working_directory, DomainError::EmptyRunWorkingDirectory)?;
            if !checkouts
                .iter()
                .any(|checkout| checkout.path == working_directory)
            {
                return Err(DomainError::InvalidDirectRunCheckout {
                    repository_id: checkouts[0].repository_id,
                });
            }
            if checkouts
                .iter()
                .find(|checkout| checkout.repository_id == repository_id)
                .map(|checkout| checkout.path.as_str())
                != Some(working_directory.as_str())
            {
                return Err(DomainError::RunWorkingDirectoryRepositoryMismatch { repository_id });
            }
            let session_name = clean_name(session_name, DomainError::EmptyRunSessionName)?;
            let pane_id = clean_name(pane_id, DomainError::EmptyRunPaneId)?;
            if state.runs.iter().any(|run| {
                run.machine_id == machine_id
                    && run.session_name == session_name
                    && run.pane_id == pane_id
            }) {
                return Err(DomainError::RunAlreadyAttached {
                    machine_id,
                    session_name,
                    pane_id,
                });
            }
            let id = state.next_run_id;
            let next_run_id = id.checked_add(1).ok_or(DomainError::SequenceExhausted)?;
            let run = Run {
                id,
                item_id,
                workspace_id: Some(workspace_id),
                repository_id: Some(repository_id),
                worktree_id: None,
                machine_id,
                agent,
                execution_profile,
                model: None,
                effort: None,
                skill_snapshot: None,
                prompt,
                working_directory,
                session_name,
                pane_id,
                started_at,
                state: RunState::Unknown,
                pane_status: RunPaneStatus::Available,
                direct_checkouts: checkouts,
                transcript: String::new(),
                grill_question_group: None,
                grill_answers: Vec::new(),
                grill_decisions: Vec::new(),
                grill_response: None,
                grill_phase: None,
                grill_action: None,
            };
            state.next_run_id = next_run_id;
            state.runs.push(run.clone());
            Ok(Decision {
                state,
                effects: vec![Effect::PersistRun { run, next_run_id }],
            })
        }
        Event::StartWorktreeRun {
            item_id,
            workspace_id,
            worktree_id,
            machine_id,
            agent,
            execution_profile,
            prompt,
            working_directory,
            session_name,
            pane_id,
            started_at,
            prompt_selection,
        } => {
            let context_id = item_context_id(&state, item_id)?;
            let workspace = state
                .workspaces
                .iter()
                .find(|workspace| workspace.id == workspace_id)
                .ok_or(DomainError::WorkspaceNotFound { workspace_id })?;
            if workspace.item_id != item_id {
                return Err(DomainError::WorkspaceItemMismatch {
                    workspace_id,
                    item_id,
                });
            }
            let worktree = state
                .worktrees
                .iter()
                .find(|worktree| worktree.id == worktree_id)
                .ok_or(DomainError::WorktreeNotFound { worktree_id })?;
            if worktree.workspace_id != workspace_id {
                return Err(DomainError::RunWorktreeWorkspaceMismatch {
                    worktree_id,
                    workspace_id,
                });
            }
            if worktree.machine_id != machine_id {
                return Err(DomainError::MachineContextMismatch {
                    machine_id,
                    context_id,
                });
            }
            let machine = state
                .machines
                .iter()
                .find(|machine| machine.id == machine_id)
                .ok_or(DomainError::MachineNotFound { machine_id })?;
            if machine.context_id != context_id {
                return Err(DomainError::MachineContextMismatch {
                    machine_id,
                    context_id,
                });
            }
            if !workspace
                .repositories
                .iter()
                .any(|repository| repository.repository_id == worktree.repository_id)
            {
                return Err(DomainError::RunRepositoryNotSelected {
                    repository_id: worktree.repository_id,
                    workspace_id,
                });
            }
            for external_object_id in prompt_selection.external_object_ids {
                if !state.links.iter().any(|link| {
                    link.item_id == item_id && link.external_object_id == external_object_id
                }) {
                    return Err(DomainError::RunPromptSourceNotLinked {
                        external_object_id,
                        item_id,
                    });
                }
            }
            let prompt = clean_name(prompt, DomainError::EmptyRunPrompt)?;
            let working_directory =
                clean_name(working_directory, DomainError::EmptyRunWorkingDirectory)?;
            if working_directory != worktree.path {
                return Err(DomainError::RunWorkingDirectoryRepositoryMismatch {
                    repository_id: worktree.repository_id,
                });
            }
            let session_name = clean_name(session_name, DomainError::EmptyRunSessionName)?;
            let pane_id = clean_name(pane_id, DomainError::EmptyRunPaneId)?;
            if state.runs.iter().any(|run| {
                run.machine_id == machine_id
                    && run.session_name == session_name
                    && run.pane_id == pane_id
            }) {
                return Err(DomainError::RunAlreadyAttached {
                    machine_id,
                    session_name,
                    pane_id,
                });
            }
            let id = state.next_run_id;
            let next_run_id = id.checked_add(1).ok_or(DomainError::SequenceExhausted)?;
            let run = Run {
                id,
                item_id,
                workspace_id: Some(workspace_id),
                repository_id: Some(worktree.repository_id),
                worktree_id: Some(worktree_id),
                machine_id,
                agent,
                execution_profile,
                model: None,
                effort: None,
                skill_snapshot: None,
                prompt,
                working_directory,
                session_name,
                pane_id,
                started_at,
                state: RunState::Unknown,
                pane_status: RunPaneStatus::Available,
                direct_checkouts: Vec::new(),
                transcript: String::new(),
                grill_question_group: None,
                grill_answers: Vec::new(),
                grill_decisions: Vec::new(),
                grill_response: None,
                grill_phase: None,
                grill_action: None,
            };
            state.next_run_id = next_run_id;
            state.runs.push(run.clone());
            Ok(Decision {
                state,
                effects: vec![Effect::PersistRun { run, next_run_id }],
            })
        }
        Event::StartGrillRun {
            item_id,
            workspace_id,
            repository_id,
            machine_id,
            configuration,
            prompt,
            skill_snapshot,
            working_directory,
            session_name,
            pane_id,
            started_at,
            checkouts,
        } => {
            let context_id = item_context_id(&state, item_id)?;
            validate_grill_configuration(&configuration)?;
            let workspace = state
                .workspaces
                .iter()
                .find(|workspace| workspace.id == workspace_id)
                .ok_or(DomainError::WorkspaceNotFound { workspace_id })?;
            if workspace.item_id != item_id {
                return Err(DomainError::WorkspaceItemMismatch {
                    workspace_id,
                    item_id,
                });
            }
            if !workspace
                .repositories
                .iter()
                .any(|repository| repository.repository_id == repository_id)
            {
                return Err(DomainError::RunRepositoryNotSelected {
                    repository_id,
                    workspace_id,
                });
            }
            let repository = state
                .repositories
                .iter()
                .find(|repository| repository.id == repository_id)
                .ok_or(DomainError::RepositoryNotFound { repository_id })?;
            let project = state
                .projects
                .iter()
                .find(|project| project.id == repository.project_id)
                .ok_or(DomainError::ProjectNotFound {
                    project_id: repository.project_id,
                })?;
            if project.context_id != context_id {
                return Err(DomainError::RepositoryProjectMismatch {
                    repository_id,
                    project_id: project.id,
                });
            }
            let machine = state
                .machines
                .iter()
                .find(|machine| machine.id == machine_id)
                .ok_or(DomainError::MachineNotFound { machine_id })?;
            if machine.context_id != context_id {
                return Err(DomainError::MachineContextMismatch {
                    machine_id,
                    context_id,
                });
            }
            if let Some(run) = state.runs.iter().find(|run| {
                run.item_id == item_id
                    && run.execution_profile == ExecutionProfile::Grill
                    && run_is_active(run)
            }) {
                return Err(DomainError::ActiveGrillRun {
                    item_id,
                    run_id: run.id,
                });
            }
            let prompt = clean_name(prompt, DomainError::EmptyRunPrompt)?;
            if skill_snapshot.trim().is_empty() {
                return Err(DomainError::EmptyGrillSkillSnapshot);
            }
            let working_directory =
                clean_name(working_directory, DomainError::EmptyRunWorkingDirectory)?;
            let selected_repository_ids = workspace
                .repositories
                .iter()
                .map(|repository| repository.repository_id)
                .collect::<Vec<_>>();
            let mut checkout_repository_ids = Vec::new();
            for checkout in &checkouts {
                if checkout.path.trim().is_empty()
                    || checkout.branch.trim().is_empty()
                    || !selected_repository_ids.contains(&checkout.repository_id)
                    || checkout_repository_ids.contains(&checkout.repository_id)
                {
                    return Err(DomainError::InvalidDirectRunCheckout {
                        repository_id: checkout.repository_id,
                    });
                }
                checkout_repository_ids.push(checkout.repository_id);
            }
            if checkout_repository_ids.len() != selected_repository_ids.len()
                || !checkouts
                    .iter()
                    .any(|checkout| checkout.path == working_directory)
            {
                return Err(DomainError::InvalidDirectRunCheckout { repository_id });
            }
            if checkouts
                .iter()
                .find(|checkout| checkout.repository_id == repository_id)
                .map(|checkout| checkout.path.as_str())
                != Some(working_directory.as_str())
            {
                return Err(DomainError::RunWorkingDirectoryRepositoryMismatch { repository_id });
            }
            let session_name = clean_name(session_name, DomainError::EmptyRunSessionName)?;
            let pane_id = clean_name(pane_id, DomainError::EmptyRunPaneId)?;
            if state.runs.iter().any(|run| {
                run.machine_id == machine_id
                    && run.session_name == session_name
                    && run.pane_id == pane_id
            }) {
                return Err(DomainError::RunAlreadyAttached {
                    machine_id,
                    session_name,
                    pane_id,
                });
            }
            let id = state.next_run_id;
            let next_run_id = id.checked_add(1).ok_or(DomainError::SequenceExhausted)?;
            let run = Run {
                id,
                item_id,
                workspace_id: Some(workspace_id),
                repository_id: Some(repository_id),
                worktree_id: None,
                machine_id,
                agent: configuration.agent,
                execution_profile: ExecutionProfile::Grill,
                model: Some(configuration.model),
                effort: Some(configuration.effort),
                skill_snapshot: Some(skill_snapshot),
                prompt,
                working_directory,
                session_name,
                pane_id,
                started_at,
                state: RunState::Unknown,
                pane_status: RunPaneStatus::Available,
                direct_checkouts: checkouts,
                transcript: String::new(),
                grill_question_group: None,
                grill_answers: Vec::new(),
                grill_decisions: Vec::new(),
                grill_response: None,
                grill_phase: Some(GrillPhase::Starting),
                grill_action: None,
            };
            state.next_run_id = next_run_id;
            state.runs.push(run.clone());
            Ok(Decision {
                state,
                effects: vec![Effect::PersistRun { run, next_run_id }],
            })
        }
        Event::AttachRun {
            item_id,
            workspace_id,
            worktree_id,
            repository_id,
            machine_id,
            agent,
            working_directory,
            session_name,
            pane_id,
            attached_at,
        } => {
            let context_id = item_context_id(&state, item_id)?;
            let workspace = state
                .workspaces
                .iter()
                .find(|workspace| workspace.id == workspace_id)
                .ok_or(DomainError::WorkspaceNotFound { workspace_id })?;
            if workspace.item_id != item_id {
                return Err(DomainError::WorkspaceItemMismatch {
                    workspace_id,
                    item_id,
                });
            }
            if !workspace
                .repositories
                .iter()
                .any(|repository| repository.repository_id == repository_id)
            {
                return Err(DomainError::RunRepositoryNotSelected {
                    repository_id,
                    workspace_id,
                });
            }
            if let Some(worktree_id) = worktree_id {
                let worktree = state
                    .worktrees
                    .iter()
                    .find(|worktree| worktree.id == worktree_id)
                    .ok_or(DomainError::WorktreeNotFound { worktree_id })?;
                if worktree.workspace_id != workspace_id
                    || worktree.repository_id != repository_id
                    || worktree.path != working_directory
                {
                    return Err(DomainError::RunWorkingDirectoryRepositoryMismatch {
                        repository_id,
                    });
                }
            } else {
                let location_matches = state.repository_locations.iter().any(|location| {
                    location.repository_id == repository_id
                        && location.machine_id == machine_id
                        && path_is_within(&location.checkout_path, &working_directory)
                });
                if !location_matches {
                    return Err(DomainError::RunWorkingDirectoryRepositoryMismatch {
                        repository_id,
                    });
                }
            }
            let machine = state
                .machines
                .iter()
                .find(|machine| machine.id == machine_id)
                .ok_or(DomainError::MachineNotFound { machine_id })?;
            if machine.context_id != context_id {
                return Err(DomainError::MachineContextMismatch {
                    machine_id,
                    context_id,
                });
            }
            let working_directory =
                clean_name(working_directory, DomainError::EmptyRunWorkingDirectory)?;
            let session_name = clean_name(session_name, DomainError::EmptyRunSessionName)?;
            let pane_id = clean_name(pane_id, DomainError::EmptyRunPaneId)?;
            if state.runs.iter().any(|run| {
                run.machine_id == machine_id
                    && run.session_name == session_name
                    && run.pane_id == pane_id
            }) {
                return Err(DomainError::RunAlreadyAttached {
                    machine_id,
                    session_name,
                    pane_id,
                });
            }
            let id = state.next_run_id;
            let next_run_id = id.checked_add(1).ok_or(DomainError::SequenceExhausted)?;
            let run = Run {
                id,
                item_id,
                workspace_id: Some(workspace_id),
                repository_id: Some(repository_id),
                worktree_id,
                machine_id,
                agent,
                execution_profile: ExecutionProfile::CustomPrompt,
                model: None,
                effort: None,
                skill_snapshot: None,
                prompt: "Attached existing agent".into(),
                working_directory,
                session_name,
                pane_id,
                started_at: attached_at,
                state: RunState::Unknown,
                pane_status: RunPaneStatus::Available,
                direct_checkouts: Vec::new(),
                transcript: String::new(),
                grill_question_group: None,
                grill_answers: Vec::new(),
                grill_decisions: Vec::new(),
                grill_response: None,
                grill_phase: None,
                grill_action: None,
            };
            state.next_run_id = next_run_id;
            state.runs.push(run.clone());
            Ok(Decision {
                state,
                effects: vec![Effect::PersistRun { run, next_run_id }],
            })
        }
        Event::UpdateRunState {
            run_id,
            state: run_state,
        } => {
            let run = state
                .runs
                .iter_mut()
                .find(|run| run.id == run_id)
                .ok_or(DomainError::RunNotFound { run_id })?;
            run.state = run_state;
            if run.execution_profile == ExecutionProfile::Grill {
                run.grill_phase = match run_state {
                    RunState::Unknown => run.grill_phase.or(Some(GrillPhase::Starting)),
                    RunState::Working => Some(GrillPhase::Working),
                    RunState::Blocked => Some(GrillPhase::WaitingForAnswers),
                    RunState::Finished => Some(GrillPhase::AwaitingNextAction),
                };
            }
            let run = run.clone();

            Ok(Decision {
                state,
                effects: vec![Effect::PersistRunState { run }],
            })
        }
        Event::FinishRun { run_id } => {
            let run = state
                .runs
                .iter_mut()
                .find(|run| run.id == run_id)
                .ok_or(DomainError::RunNotFound { run_id })?;
            run.state = RunState::Finished;
            if run.execution_profile == ExecutionProfile::Grill {
                run.grill_phase = Some(GrillPhase::Finished);
            }
            let run = run.clone();

            Ok(Decision {
                state,
                effects: vec![Effect::PersistRunState { run }],
            })
        }
        Event::ContinueGrill { run_id, action } => {
            let run = state
                .runs
                .iter_mut()
                .find(|run| run.id == run_id)
                .ok_or(DomainError::RunNotFound { run_id })?;
            if run.execution_profile != ExecutionProfile::Grill
                || run.grill_phase != Some(GrillPhase::AwaitingNextAction)
                || run.pane_status != RunPaneStatus::Available
            {
                return Err(DomainError::GrillContinuationNotAvailable {
                    run_id,
                    phase: run.grill_phase,
                });
            }
            run.state = RunState::Working;
            run.grill_phase = Some(GrillPhase::Working);
            run.grill_question_group = None;
            run.grill_answers.clear();
            run.grill_response = None;
            run.grill_action = Some(action);
            let run = run.clone();

            Ok(Decision {
                state,
                effects: vec![Effect::PersistRunState { run }],
            })
        }
        Event::CaptureDownstreamIssues {
            run_id,
            action,
            issues,
        } => {
            let run = state
                .runs
                .iter()
                .find(|run| run.id == run_id)
                .cloned()
                .ok_or(DomainError::RunNotFound { run_id })?;
            if run.execution_profile != ExecutionProfile::Grill
                || run.grill_action != Some(action)
                || !matches!(
                    action,
                    GrillContinuationAction::ToSpec | GrillContinuationAction::ToTickets
                )
            {
                return Err(DomainError::DownstreamCaptureNotAvailable { run_id, action });
            }

            let mut effects = Vec::new();
            for issue in issues {
                if issue.object.provider != ExternalProvider::GitHub
                    || issue.object.kind != ExternalObjectKind::Issue
                    || issue.object.external_key.trim().is_empty()
                    || issue.object.canonical_url.trim().is_empty()
                {
                    continue;
                }
                effects.extend(link_external_object(
                    &mut state,
                    run.item_id,
                    issue.object,
                    Some(issue.snapshot),
                    Some(LinkProvenance {
                        run_id,
                        action,
                        discovery: issue.discovery,
                    }),
                    true,
                )?);
            }

            Ok(Decision { state, effects })
        }
        Event::RecordRunTranscript {
            run_id,
            transcript,
            question_group,
        } => {
            let run = state
                .runs
                .iter_mut()
                .find(|run| run.id == run_id)
                .ok_or(DomainError::RunNotFound { run_id })?;
            if run.execution_profile != ExecutionProfile::Grill {
                return Err(DomainError::NotGrillRun { run_id });
            }
            run.transcript = transcript;
            if run.grill_question_group != question_group {
                run.grill_answers.clear();
                run.grill_response = None;
                run.grill_question_group = question_group;
            }
            let run = run.clone();

            Ok(Decision {
                state,
                effects: vec![Effect::PersistRunTranscript { run }],
            })
        }
        Event::RecordGrillAnswers { run_id, answers } => {
            let run = state
                .runs
                .iter_mut()
                .find(|run| run.id == run_id)
                .ok_or(DomainError::RunNotFound { run_id })?;
            if run.execution_profile != ExecutionProfile::Grill {
                return Err(DomainError::NotGrillRun { run_id });
            }
            if run.grill_response.is_some() {
                return Err(DomainError::GrillResponseAlreadySubmitted { run_id });
            }
            let question_group = run
                .grill_question_group
                .as_ref()
                .ok_or(DomainError::GrillQuestionGroupNotFound { run_id })?;
            let mut normalized_answers = answers;
            normalized_answers.sort_by_key(|answer| answer.question_number);
            if normalized_answers.len() != question_group.questions.len() {
                return Err(DomainError::GrillQuestionGroupNotFound { run_id });
            }
            for answer in &normalized_answers {
                if !question_group
                    .questions
                    .iter()
                    .any(|question| question.number == answer.question_number)
                {
                    return Err(DomainError::UnknownGrillQuestion {
                        question_number: answer.question_number,
                    });
                }
            }
            format_grill_response(&normalized_answers)?;
            run.grill_decisions
                .extend(normalized_answers.iter().cloned());
            run.grill_answers = normalized_answers;
            let run = run.clone();

            Ok(Decision {
                state,
                effects: vec![Effect::PersistGrillAnswers { run }],
            })
        }
        Event::RecordGrillResponse { run_id, response } => {
            let run = state
                .runs
                .iter_mut()
                .find(|run| run.id == run_id)
                .ok_or(DomainError::RunNotFound { run_id })?;
            if run.execution_profile != ExecutionProfile::Grill {
                return Err(DomainError::NotGrillRun { run_id });
            }
            if run.grill_answers.is_empty() {
                return Err(DomainError::EmptyGrillAnswer);
            }
            let response = clean_name(response, DomainError::EmptyGrillResponse)?;
            run.grill_response = Some(response);
            let run = run.clone();

            Ok(Decision {
                state,
                effects: vec![Effect::PersistGrillResponse { run }],
            })
        }
        Event::SetRunPaneStatus { run_id, status } => {
            let run = state
                .runs
                .iter_mut()
                .find(|run| run.id == run_id)
                .ok_or(DomainError::RunNotFound { run_id })?;
            run.pane_status = status;
            if run.execution_profile == ExecutionProfile::Grill {
                if status != RunPaneStatus::Available && run_is_active(run) {
                    run.grill_phase = Some(GrillPhase::RecoverablePaneLoss);
                } else if status == RunPaneStatus::Available
                    && (run.grill_phase.is_none()
                        || run.grill_phase == Some(GrillPhase::RecoverablePaneLoss))
                {
                    run.grill_phase = Some(match run.state {
                        RunState::Unknown => GrillPhase::Starting,
                        RunState::Working => GrillPhase::Working,
                        RunState::Blocked => GrillPhase::WaitingForAnswers,
                        RunState::Finished => GrillPhase::AwaitingNextAction,
                    });
                }
            }
            let run = run.clone();

            Ok(Decision {
                state,
                effects: vec![Effect::PersistRunPaneStatus { run }],
            })
        }
        Event::SetItemStatus { item_id, status } => {
            let item = state
                .items
                .iter_mut()
                .find(|item| item.id == item_id)
                .ok_or(DomainError::ItemNotFound { item_id })?;
            item.status = status;
            let item = item.clone();

            Ok(Decision {
                state,
                effects: vec![Effect::PersistItemUpdate { item }],
            })
        }
        Event::SetItemTitle { item_id, title } => {
            let title = title.trim();
            if title.is_empty() {
                return Err(DomainError::EmptyTitle);
            }
            let item = state
                .items
                .iter_mut()
                .find(|item| item.id == item_id)
                .ok_or(DomainError::ItemNotFound { item_id })?;
            item.title = title.to_owned();
            let item = item.clone();

            Ok(Decision {
                state,
                effects: vec![Effect::PersistItemUpdate { item }],
            })
        }
        Event::SetItemNotes { item_id, notes } => {
            let item = state
                .items
                .iter_mut()
                .find(|item| item.id == item_id)
                .ok_or(DomainError::ItemNotFound { item_id })?;
            item.notes = notes;
            let item = item.clone();

            Ok(Decision {
                state,
                effects: vec![Effect::PersistItemUpdate { item }],
            })
        }
        Event::SetItemRelation {
            from_item_id,
            to_item_id,
            kind,
        } => {
            if from_item_id == to_item_id {
                return Err(DomainError::SelfRelation {
                    item_id: from_item_id,
                });
            }
            let from_context_id = item_context_id(&state, from_item_id)?;
            let to_context_id = item_context_id(&state, to_item_id)?;
            if from_context_id != to_context_id {
                return Err(DomainError::ItemContextMismatch {
                    from_item_id,
                    to_item_id,
                });
            }

            let relation = ItemRelation {
                from_item_id,
                to_item_id,
                kind,
            };
            if state.relationships.contains(&relation) {
                return Err(DomainError::RelationAlreadyExists);
            }
            state.relationships.push(relation.clone());

            Ok(Decision {
                state,
                effects: vec![Effect::PersistItemRelation { relation }],
            })
        }
        Event::AddItemReminder { item_id, remind_at } => {
            let remind_at = clean_name(remind_at, DomainError::EmptyReminderAt)?;
            let item = state
                .items
                .iter_mut()
                .find(|item| item.id == item_id)
                .ok_or(DomainError::ItemNotFound { item_id })?;
            let id = state.next_reminder_id;
            let next_reminder_id = id.checked_add(1).ok_or(DomainError::SequenceExhausted)?;
            item.reminders.push(Reminder { id, remind_at });
            state.next_reminder_id = next_reminder_id;
            let item = item.clone();

            Ok(Decision {
                state,
                effects: vec![Effect::PersistItemReminders {
                    item,
                    next_reminder_id,
                }],
            })
        }
        Event::RemoveItemReminder {
            item_id,
            reminder_id,
        } => {
            let item = state
                .items
                .iter_mut()
                .find(|item| item.id == item_id)
                .ok_or(DomainError::ItemNotFound { item_id })?;
            let position = item
                .reminders
                .iter()
                .position(|reminder| reminder.id == reminder_id)
                .ok_or(DomainError::ReminderNotFound {
                    item_id,
                    reminder_id,
                })?;
            item.reminders.remove(position);
            let item = item.clone();
            let next_reminder_id = state.next_reminder_id;

            Ok(Decision {
                state,
                effects: vec![Effect::PersistItemReminders {
                    item,
                    next_reminder_id,
                }],
            })
        }
        Event::SetLinkWatchUntil {
            link_id,
            watch_until,
        } => {
            let link = state
                .links
                .iter_mut()
                .find(|link| link.id == link_id)
                .ok_or(DomainError::LinkNotFound { link_id })?;
            link.watch_until = watch_until;
            let link = link.clone();

            Ok(Decision {
                state,
                effects: vec![Effect::PersistLinkState { link }],
            })
        }
        Event::SetLinkReviewAt { link_id, review_at } => {
            let link = state
                .links
                .iter_mut()
                .find(|link| link.id == link_id)
                .ok_or(DomainError::LinkNotFound { link_id })?;
            link.review_at = review_at;
            let link = link.clone();

            Ok(Decision {
                state,
                effects: vec![Effect::PersistLinkState { link }],
            })
        }
        Event::ClearLinkReviewAt { link_id } => {
            let link = state
                .links
                .iter_mut()
                .find(|link| link.id == link_id)
                .ok_or(DomainError::LinkNotFound { link_id })?;
            link.review_at = None;
            let link = link.clone();

            Ok(Decision {
                state,
                effects: vec![Effect::PersistLinkState { link }],
            })
        }
        Event::LinkExternalObject {
            item_id,
            object,
            snapshot,
        } => {
            let effects = link_external_object(&mut state, item_id, object, snapshot, None, false)?;
            Ok(Decision { state, effects })
        }
        Event::RefreshExternalObject {
            external_object_id,
            snapshot: snapshot_data,
        } => {
            if !state
                .external_objects
                .iter()
                .any(|object| object.id == external_object_id)
            {
                return Err(DomainError::ExternalObjectNotFound { external_object_id });
            }
            let snapshot = ExternalSnapshot {
                external_object_id,
                title: snapshot_data.title,
                state: snapshot_data.state,
                metadata: snapshot_data.metadata,
                fetched_at: snapshot_data.fetched_at,
            };
            let changes = state
                .snapshots
                .iter()
                .find(|existing| existing.external_object_id == external_object_id)
                .map(|previous| snapshot_changes(previous, &snapshot))
                .unwrap_or_default();
            upsert_snapshot(&mut state, snapshot.clone());

            let mut effects = Vec::new();
            if !changes.is_empty() {
                let id = state.next_activity_id;
                let next_activity_id = id.checked_add(1).ok_or(DomainError::SequenceExhausted)?;
                let activity = Activity {
                    id,
                    external_object_id,
                    observed_at: snapshot.fetched_at,
                    changes,
                };
                state.next_activity_id = next_activity_id;
                state.activities.push(activity.clone());
                effects.push(Effect::PersistActivity {
                    activity,
                    next_activity_id,
                });
            }
            effects.push(Effect::PersistExternalSnapshot { snapshot });

            Ok(Decision { state, effects })
        }
        Event::SetLinkAttentionPolicy { link_id, policy } => {
            let link = state
                .links
                .iter_mut()
                .find(|link| link.id == link_id)
                .ok_or(DomainError::LinkNotFound { link_id })?;
            link.attention_policy = policy;
            let link = link.clone();

            Ok(Decision {
                state,
                effects: vec![Effect::PersistLinkState { link }],
            })
        }
        Event::SetContextAttentionDefault {
            context_id,
            object_kind,
            policy,
        } => {
            ensure_context(&state, context_id)?;
            let attention_default = ContextAttentionDefault {
                context_id,
                object_kind,
                policy,
            };
            if let Some(existing) = state.attention_defaults.iter_mut().find(|existing| {
                existing.context_id == context_id && existing.object_kind == object_kind
            }) {
                *existing = attention_default.clone();
            } else {
                state.attention_defaults.push(attention_default.clone());
            }

            Ok(Decision {
                state,
                effects: vec![Effect::PersistContextAttentionDefault { attention_default }],
            })
        }
        Event::MarkLinkReviewed { link_id } => {
            let external_object_id = state
                .links
                .iter()
                .find(|link| link.id == link_id)
                .ok_or(DomainError::LinkNotFound { link_id })?
                .external_object_id;
            let reviewed_activity_id = state
                .activities
                .iter()
                .filter(|activity| activity.external_object_id == external_object_id)
                .map(|activity| activity.id)
                .max()
                .unwrap_or_default();
            let link = state
                .links
                .iter_mut()
                .find(|link| link.id == link_id)
                .expect("the Link was checked above");
            link.reviewed_activity_id = reviewed_activity_id;
            let link = link.clone();

            Ok(Decision {
                state,
                effects: vec![Effect::PersistLinkState { link }],
            })
        }
    }
}

pub fn suggest_untracked_runs(
    state: &DomainState,
    panes: &[AgentPaneObservation],
) -> Vec<RunSuggestion> {
    let mut suggestions = panes
        .iter()
        .filter(|pane| {
            !state.runs.iter().any(|run| {
                run.machine_id == pane.machine_id
                    && run.session_name == pane.session_name
                    && run.pane_id == pane.pane_id
            })
        })
        .filter_map(|pane| {
            let machine = state
                .machines
                .iter()
                .find(|machine| machine.id == pane.machine_id)?;
            let workspace_location = state
                .workspaces
                .iter()
                .filter_map(|workspace| {
                    let worktree = state
                        .worktrees
                        .iter()
                        .filter(|worktree| {
                            worktree.workspace_id == workspace.id
                                && worktree.machine_id == pane.machine_id
                                && path_is_within(&worktree.path, &pane.current_path)
                        })
                        .max_by_key(|worktree| worktree.path.len());
                    if let Some(worktree) = worktree {
                        return Some((
                            worktree.path.len(),
                            workspace.id,
                            worktree.repository_id,
                            Some(worktree.id),
                            worktree.path.clone(),
                        ));
                    }
                    workspace.repositories.iter().find_map(|selected| {
                        state
                            .repository_locations
                            .iter()
                            .filter(|location| {
                                location.repository_id == selected.repository_id
                                    && location.machine_id == pane.machine_id
                                    && path_is_within(&location.checkout_path, &pane.current_path)
                            })
                            .max_by_key(|location| location.checkout_path.len())
                            .map(|location| {
                                (
                                    location.checkout_path.len(),
                                    workspace.id,
                                    selected.repository_id,
                                    None,
                                    location.checkout_path.clone(),
                                )
                            })
                    })
                })
                .max_by_key(|candidate| candidate.0);
            let (_, workspace_id, repository_id, worktree_id, location_path) = workspace_location?;
            let workspace = state
                .workspaces
                .iter()
                .find(|workspace| workspace.id == workspace_id)?;
            let item_id = workspace.item_id;
            let item = state.items.iter().find(|item| item.id == item_id)?;
            let project = state
                .projects
                .iter()
                .find(|project| project.id == item.project_id)?;
            let context = state
                .contexts
                .iter()
                .find(|context| context.id == project.context_id)?;

            Some(RunSuggestion {
                machine_id: machine.id,
                machine_name: machine.name.clone(),
                agent: pane.agent,
                session_name: pane.session_name.clone(),
                pane_id: pane.pane_id.clone(),
                current_path: pane.current_path.clone(),
                item_id: item.id,
                item_identifier: item.human_identifier.clone(),
                item_title: item.title.clone(),
                context_id: context.id,
                context_name: context.name.clone(),
                workspace_id: Some(workspace_id),
                repository_id: Some(repository_id),
                worktree_id,
                location_path: Some(location_path),
            })
        })
        .collect::<Vec<_>>();
    suggestions.sort_by(|left, right| {
        left.machine_id
            .cmp(&right.machine_id)
            .then_with(|| left.session_name.cmp(&right.session_name))
            .then_with(|| left.pane_id.cmp(&right.pane_id))
    });
    suggestions
}

fn path_is_within(root: &str, path: &str) -> bool {
    let root = without_macos_private_prefix(root).trim_end_matches('/');
    let path = without_macos_private_prefix(path);
    if root == "/" {
        return true;
    }

    if root == "~" || root.starts_with("~/") {
        if same_or_descendant_path(root, path) {
            return true;
        }

        let Some(home) = std::env::var("HOME").ok() else {
            return false;
        };
        let home = without_macos_private_prefix(&home).trim_end_matches('/');
        if home.is_empty() {
            return false;
        }
        let Some(relative_path) = path.strip_prefix(home) else {
            return false;
        };
        if !relative_path.is_empty() && !relative_path.starts_with('/') {
            return false;
        }
        let home_relative_path = format!("~{relative_path}");
        return same_or_descendant_path(root, &home_relative_path);
    }

    same_or_descendant_path(root, path)
}

fn same_or_descendant_path(root: &str, path: &str) -> bool {
    path == root
        || path
            .strip_prefix(root)
            .is_some_and(|suffix| suffix.starts_with('/'))
}

fn without_macos_private_prefix(path: &str) -> &str {
    path.strip_prefix("/private")
        .filter(|path| path.starts_with('/'))
        .unwrap_or(path)
}

pub fn home_view(state: &DomainState, context_id: Option<i64>, now: &str) -> HomeView {
    let mut view = HomeView {
        needs_attention: Vec::new(),
        attention_entries: attention_entries(state, context_id, now),
        running: Vec::new(),
        waiting: Vec::new(),
        due: Vec::new(),
        completed: Vec::new(),
    };

    for item in item_views_at(state, context_id, Some(now)) {
        let is_due = item_has_due_reminder(&item.item, now) && item.item.status != ItemStatus::Done;
        if is_due {
            view.due.push(item.clone());
        }
        if is_due
            || item.item.status == ItemStatus::Inbox
            || (item.item.status != ItemStatus::Done
                && view
                    .attention_entries
                    .iter()
                    .any(|entry| entry.item_id == item.item.id))
        {
            view.needs_attention.push(item.clone());
        }
        match item.item.status {
            ItemStatus::Inbox => {}
            ItemStatus::Active => view.running.push(item),
            ItemStatus::Waiting => view.waiting.push(item),
            ItemStatus::Done => view.completed.push(item),
        }
    }

    view
}

pub fn search_items(state: &DomainState, query: &str, context_id: Option<i64>) -> Vec<ItemView> {
    let query = query.trim().to_lowercase();
    item_views(state, context_id)
        .into_iter()
        .filter(|view| {
            query.is_empty()
                || [
                    view.item.human_identifier.as_str(),
                    view.item.title.as_str(),
                    view.item.notes.as_str(),
                    view.context_name.as_str(),
                    view.project_name.as_str(),
                ]
                .iter()
                .any(|field| field.to_lowercase().contains(&query))
        })
        .collect()
}

pub fn external_link_view(state: &DomainState, link: &Link) -> Option<ExternalLinkView> {
    external_link_view_at(state, link, None)
}

fn external_link_view_at(
    state: &DomainState,
    link: &Link,
    now: Option<&str>,
) -> Option<ExternalLinkView> {
    let object = state
        .external_objects
        .iter()
        .find(|object| object.id == link.external_object_id)?;
    let snapshot = state
        .snapshots
        .iter()
        .find(|snapshot| snapshot.external_object_id == object.id)
        .cloned();
    Some(ExternalLinkView {
        link: link.clone(),
        object: object.clone(),
        snapshot,
        attention_policy: effective_attention_policy(state, link, object),
        attention_entry: attention_entry_for_link_at(state, link, object, now),
    })
}

pub fn attention_entries(
    state: &DomainState,
    context_id: Option<i64>,
    now: &str,
) -> Vec<AttentionEntry> {
    attention_entries_at(state, context_id, Some(now))
}

fn attention_entries_at(
    state: &DomainState,
    context_id: Option<i64>,
    now: Option<&str>,
) -> Vec<AttentionEntry> {
    let mut entries = Vec::new();
    for link in state.links.iter().filter(|link| {
        context_id.is_none_or(|context_id| {
            item_context_id(state, link.item_id)
                .map(|link_context_id| link_context_id == context_id)
                .unwrap_or(false)
        })
    }) {
        let object = state
            .external_objects
            .iter()
            .find(|object| object.id == link.external_object_id);
        let Some(object) = object else {
            continue;
        };
        if let Some(entry) = attention_entry_for_link_at(state, link, object, now) {
            entries.push(entry);
        }
        if link
            .review_at
            .as_deref()
            .is_some_and(|review_at| now.is_some_and(|now| review_at <= now))
        {
            entries.push(AttentionEntry {
                kind: AttentionEntryKind::Review,
                link_id: link.id,
                reminder_id: None,
                run_id: None,
                item_id: link.item_id,
                external_object_id: object.id,
                source_title: state
                    .snapshots
                    .iter()
                    .find(|snapshot| snapshot.external_object_id == object.id)
                    .map(|snapshot| snapshot.title.clone())
                    .unwrap_or_else(|| object.canonical_url.clone()),
                source_url: object.canonical_url.clone(),
                activities: Vec::new(),
                summary: format!(
                    "Review scheduled for {}",
                    link.review_at.as_deref().unwrap_or_default()
                ),
            });
        }
    }

    for item in state.items.iter().filter(|item| {
        context_id.is_none_or(|context_id| {
            item_context_id(state, item.id)
                .map(|item_context_id| item_context_id == context_id)
                .unwrap_or(false)
        })
    }) {
        entries.extend(
            item.reminders
                .iter()
                .filter(|reminder| reminder.remind_at.as_str() <= now.unwrap_or_default())
                .map(|reminder| AttentionEntry {
                    kind: AttentionEntryKind::Reminder,
                    link_id: 0,
                    reminder_id: Some(reminder.id),
                    run_id: None,
                    item_id: item.id,
                    external_object_id: 0,
                    source_title: item.title.clone(),
                    source_url: String::new(),
                    activities: Vec::new(),
                    summary: format!("Reminder due at {}", reminder.remind_at),
                }),
        );
    }

    entries.extend(
        state
            .runs
            .iter()
            .filter(|run| run.state == RunState::Blocked)
            .filter(|run| {
                context_id.is_none_or(|context_id| {
                    item_context_id(state, run.item_id)
                        .map(|run_context_id| run_context_id == context_id)
                        .unwrap_or(false)
                })
            })
            .filter_map(|run| {
                let item = state.items.iter().find(|item| item.id == run.item_id)?;
                Some(AttentionEntry {
                    kind: AttentionEntryKind::BlockedRun,
                    link_id: 0,
                    reminder_id: None,
                    run_id: Some(run.id),
                    item_id: item.id,
                    external_object_id: 0,
                    source_title: item.title.clone(),
                    source_url: String::new(),
                    activities: Vec::new(),
                    summary: format!("Run #{} is blocked and needs your input", run.id),
                })
            }),
    );

    entries
}

fn item_has_due_reminder(item: &Item, now: &str) -> bool {
    item.reminders
        .iter()
        .any(|reminder| reminder.remind_at.as_str() <= now)
}

fn item_views(state: &DomainState, context_id: Option<i64>) -> Vec<ItemView> {
    item_views_at(state, context_id, None)
}

fn item_views_at(state: &DomainState, context_id: Option<i64>, now: Option<&str>) -> Vec<ItemView> {
    state
        .items
        .iter()
        .filter_map(|item| {
            let project = state
                .projects
                .iter()
                .find(|project| project.id == item.project_id)?;
            if context_id.is_some_and(|candidate| candidate != project.context_id) {
                return None;
            }
            let context = state
                .contexts
                .iter()
                .find(|context| context.id == project.context_id)?;
            let relationships = state
                .relationships
                .iter()
                .filter(|relation| {
                    relation.from_item_id == item.id || relation.to_item_id == item.id
                })
                .cloned()
                .collect();
            let links = state
                .links
                .iter()
                .filter(|link| link.item_id == item.id)
                .filter_map(|link| external_link_view_at(state, link, now))
                .collect();
            let workspaces = state
                .workspaces
                .iter()
                .filter(|workspace| workspace.item_id == item.id)
                .cloned()
                .collect();
            let runs = state
                .runs
                .iter()
                .filter(|run| run.item_id == item.id)
                .cloned()
                .collect();
            let worktrees = state
                .worktrees
                .iter()
                .filter(|worktree| {
                    state.workspaces.iter().any(|workspace| {
                        workspace.id == worktree.workspace_id && workspace.item_id == item.id
                    })
                })
                .cloned()
                .collect();
            Some(ItemView {
                item: item.clone(),
                context_id: context.id,
                context_name: context.name.clone(),
                project_name: project.name.clone(),
                relationships,
                workspaces,
                worktrees,
                runs,
                links,
            })
        })
        .collect()
}

pub fn compose_run_prompt(
    state: &DomainState,
    item_id: i64,
    profile: ExecutionProfile,
    selection: &RunPromptSelection,
    custom_prompt: Option<&str>,
) -> Result<String, DomainError> {
    let item = state
        .items
        .iter()
        .find(|item| item.id == item_id)
        .ok_or(DomainError::ItemNotFound { item_id })?;
    let mut sections = Vec::new();
    if selection.include_objective {
        sections.push(format!("Item objective:\n{}", item.title));
    }
    if selection.include_notes && !item.notes.trim().is_empty() {
        sections.push(format!("Item notes:\n{}", item.notes.trim()));
    }
    for external_object_id in &selection.external_object_ids {
        let link = state
            .links
            .iter()
            .find(|link| link.item_id == item_id && link.external_object_id == *external_object_id)
            .ok_or(DomainError::RunPromptSourceNotLinked {
                external_object_id: *external_object_id,
                item_id,
            })?;
        let object = state
            .external_objects
            .iter()
            .find(|object| object.id == link.external_object_id)
            .ok_or(DomainError::ExternalObjectNotFound {
                external_object_id: *external_object_id,
            })?;
        let title = state
            .snapshots
            .iter()
            .find(|snapshot| snapshot.external_object_id == object.id)
            .map(|snapshot| snapshot.title.as_str())
            .unwrap_or("Linked external object");
        sections.push(format!("Linked source:\n{title}\n{}", object.canonical_url));
    }

    let instruction = match profile {
        ExecutionProfile::Investigate => {
            "Investigate this work, inspect the relevant code, and report findings before changing files."
                .to_owned()
        }
        ExecutionProfile::Implement => {
            "Implement this work in the selected Workspace, run the relevant checks, and leave the changes ready for review."
                .to_owned()
        }
        ExecutionProfile::Review => {
            "Review the current Workspace changes for correctness, regressions, and missing test coverage."
                .to_owned()
        }
        ExecutionProfile::CustomPrompt => clean_name(
            custom_prompt.unwrap_or_default().to_owned(),
            DomainError::EmptyRunPrompt,
        )?,
        ExecutionProfile::Grill => {
            "Use the selected grilling skill to ask a structured frontier of questions before recommending the next decision."
                .to_owned()
        }
    };
    sections.insert(0, instruction);
    let prompt = sections.join("\n\n");
    clean_name(prompt, DomainError::EmptyRunPrompt)
}

pub fn compose_grill_prompt(
    state: &DomainState,
    item_id: i64,
    configuration: &GrillConfiguration,
    initial_prompt: &str,
) -> Result<String, DomainError> {
    validate_grill_configuration(configuration)?;
    let item = state
        .items
        .iter()
        .find(|item| item.id == item_id)
        .ok_or(DomainError::ItemNotFound { item_id })?;
    let initial_prompt = clean_name(initial_prompt.to_owned(), DomainError::EmptyRunPrompt)?;
    let mut context = vec![format!("Item objective:\n{}", item.title)];
    if !item.notes.trim().is_empty() {
        context.push(format!("Item notes:\n{}", item.notes.trim()));
    }
    for link in state.links.iter().filter(|link| link.item_id == item_id) {
        if let Some(object) = state
            .external_objects
            .iter()
            .find(|object| object.id == link.external_object_id)
        {
            let title = state
                .snapshots
                .iter()
                .find(|snapshot| snapshot.external_object_id == object.id)
                .map(|snapshot| snapshot.title.as_str())
                .unwrap_or("Linked external object");
            context.push(format!("Linked source:\n{title}\n{}", object.canonical_url));
        }
    }
    Ok(format!(
        "You are starting a Grill Run.\n\nGrill configuration: agent={}, model={}, effort={}.\n\nGrilling skill snapshot:\n{}\n\nRelevant Item context:\n{}\n\nUser's initial prompt:\n{}",
        serde_json::to_string(&configuration.agent).unwrap_or_else(|_| "unknown".into()),
        configuration.model,
        configuration.effort,
        GRILL_SKILL_SNAPSHOT,
        context.join("\n\n"),
        initial_prompt,
    ))
}

pub fn compose_grill_continuation_prompt(
    state: &DomainState,
    run_id: i64,
    action: GrillContinuationAction,
) -> Result<String, DomainError> {
    let run = state
        .runs
        .iter()
        .find(|run| run.id == run_id)
        .ok_or(DomainError::RunNotFound { run_id })?;
    if run.execution_profile != ExecutionProfile::Grill {
        return Err(DomainError::NotGrillRun { run_id });
    }
    let item = state
        .items
        .iter()
        .find(|item| item.id == run.item_id)
        .ok_or(DomainError::ItemNotFound {
            item_id: run.item_id,
        })?;
    let mut context = vec![format!("Item objective:\n{}", item.title)];
    if !item.notes.trim().is_empty() {
        context.push(format!("Item notes:\n{}", item.notes.trim()));
    }
    for link in state.links.iter().filter(|link| link.item_id == item.id) {
        if let Some(object) = state
            .external_objects
            .iter()
            .find(|object| object.id == link.external_object_id)
        {
            let title = state
                .snapshots
                .iter()
                .find(|snapshot| snapshot.external_object_id == object.id)
                .map(|snapshot| snapshot.title.as_str())
                .unwrap_or("Linked external object");
            context.push(format!("Linked source:\n{title}\n{}", object.canonical_url));
        }
    }
    let decisions = if run.grill_decisions.is_empty() {
        run.grill_response
            .as_deref()
            .filter(|response| !response.trim().is_empty())
            .map(str::to_owned)
            .or_else(|| format_grill_response(&run.grill_answers).ok())
            .unwrap_or_else(|| "No structured Grill decisions were recorded.".into())
    } else {
        format_recorded_grill_decisions(&run.grill_decisions)
    };

    Ok(format!(
        "Continue the existing Grill Run in the same Run and Pane.\n\nSelected downstream action: {}\n\nDownstream skill snapshot (inject this content explicitly; do not rely on the agent having the skill installed):\n{}\n\nRelevant Item context:\n{}\n\nGrill transcript:\n{}\n\nRecorded Grill decisions:\n{}\n\nContinuation instruction:\nApply the selected {} skill to the Item using the transcript and decisions above. Keep working in the same Workspace and working directory. Ask any confirmation questions using the same ❓ and ➡️ markers as one grouped frontier. Wait for an explicit user decision to finish or stop; never mark the Item Done automatically.\n\nWhen to-spec or to-tickets creates a GitHub Issue, emit one machine-readable line with the confirmed Issue URL: AI_MISSION_MANAGER_EVENT {{\"event\":\"github.issue.created\",\"url\":\"<canonical URL>\",\"run_id\":{},\"action\":\"{}\"}}",
        action.as_str(),
        action.skill_snapshot(),
        context.join("\n\n"),
        if run.transcript.trim().is_empty() {
            "No transcript was captured yet."
        } else {
            run.transcript.as_str()
        },
        decisions,
        action.as_str(),
        run.id,
        action.as_str(),
    ))
}

fn format_recorded_grill_decisions(decisions: &[GrillAnswer]) -> String {
    decisions
        .iter()
        .map(|decision| {
            let answer = decision.answer.trim().replace('\n', "\n   ");
            format!("Q{}: {answer}", decision.question_number)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn clean_name<E>(name: String, empty_error: E) -> Result<String, E> {
    let name = name.trim();
    if name.is_empty() {
        return Err(empty_error);
    }
    Ok(name.to_owned())
}

fn clean_machine_transport(transport: MachineTransport) -> Result<MachineTransport, DomainError> {
    match transport {
        MachineTransport::Local => Ok(MachineTransport::Local),
        MachineTransport::Ssh {
            host,
            user,
            port,
            identity_file,
            known_hosts_file,
            strict_host_key_checking,
        } => {
            let host = host.trim().to_owned();
            if host.is_empty() {
                return Err(DomainError::EmptyMachineHost);
            }
            if !host
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b".@:_-".contains(&byte))
            {
                return Err(DomainError::InvalidMachineHost);
            }
            let user = user.map(|value| value.trim().to_owned());
            if user.as_deref().is_some_and(str::is_empty) {
                return Err(DomainError::EmptyMachineUser);
            }
            if user.as_deref().is_some_and(|value| {
                !value
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
            }) {
                return Err(DomainError::InvalidMachineUser);
            }
            if port == Some(0) {
                return Err(DomainError::InvalidMachinePort);
            }
            if strict_host_key_checking
                .as_deref()
                .is_some_and(|value| !matches!(value, "yes" | "accept-new" | "no"))
            {
                return Err(DomainError::InvalidMachineHostKeyChecking);
            }
            Ok(MachineTransport::Ssh {
                host,
                user,
                port,
                identity_file: identity_file.map(|value| value.trim().to_owned()),
                known_hosts_file: known_hosts_file.map(|value| value.trim().to_owned()),
                strict_host_key_checking,
            })
        }
    }
}

fn ensure_context(state: &DomainState, context_id: i64) -> Result<(), DomainError> {
    if state
        .contexts
        .iter()
        .any(|context| context.id == context_id)
    {
        Ok(())
    } else {
        Err(DomainError::ContextNotFound { context_id })
    }
}

fn item_project_id(state: &DomainState, item_id: i64) -> Result<i64, DomainError> {
    let item = state
        .items
        .iter()
        .find(|item| item.id == item_id)
        .ok_or(DomainError::ItemNotFound { item_id })?;
    state
        .projects
        .iter()
        .find(|project| project.id == item.project_id)
        .map(|project| project.id)
        .ok_or(DomainError::ProjectNotFound {
            project_id: item.project_id,
        })
}

fn normalize_workspace_repositories(
    state: &DomainState,
    project_id: i64,
    repositories: Vec<WorkspaceRepositoryInput>,
) -> Result<Vec<WorkspaceRepository>, DomainError> {
    let mut normalized = Vec::with_capacity(repositories.len());
    for input in repositories {
        let repository = state
            .repositories
            .iter()
            .find(|repository| repository.id == input.repository_id)
            .ok_or(DomainError::RepositoryNotFound {
                repository_id: input.repository_id,
            })?;
        if repository.project_id != project_id {
            return Err(DomainError::RepositoryProjectMismatch {
                repository_id: input.repository_id,
                project_id,
            });
        }
        if normalized
            .iter()
            .any(|selected: &WorkspaceRepository| selected.repository_id == input.repository_id)
        {
            return Err(DomainError::DuplicateRepositorySelection {
                repository_id: input.repository_id,
            });
        }
        normalized.push(WorkspaceRepository {
            repository_id: input.repository_id,
            branch: clean_name(input.branch, DomainError::EmptyBranch)?,
            base_branch: clean_name(input.base_branch, DomainError::EmptyBranch)?,
        });
    }
    Ok(normalized)
}

fn clean_repository_name(name: String) -> Result<String, DomainError> {
    let name = clean_name(name, DomainError::EmptyRepositoryName)?;
    if name == "." || name == ".." || name.contains('/') || name.contains('\\') {
        return Err(DomainError::InvalidRepositoryName);
    }
    Ok(name)
}

fn item_context_id(state: &DomainState, item_id: i64) -> Result<i64, DomainError> {
    let item = state
        .items
        .iter()
        .find(|item| item.id == item_id)
        .ok_or(DomainError::ItemNotFound { item_id })?;
    state
        .projects
        .iter()
        .find(|project| project.id == item.project_id)
        .map(|project| project.context_id)
        .ok_or(DomainError::ProjectNotFound {
            project_id: item.project_id,
        })
}

fn ensure_item(state: &DomainState, item_id: i64) -> Result<(), DomainError> {
    if state.items.iter().any(|item| item.id == item_id) {
        Ok(())
    } else {
        Err(DomainError::ItemNotFound { item_id })
    }
}

fn upsert_snapshot(state: &mut DomainState, snapshot: ExternalSnapshot) {
    if let Some(existing) = state
        .snapshots
        .iter_mut()
        .find(|existing| existing.external_object_id == snapshot.external_object_id)
    {
        *existing = snapshot;
    } else {
        state.snapshots.push(snapshot);
    }
}

fn link_external_object(
    state: &mut DomainState,
    item_id: i64,
    object: ExternalObjectInput,
    snapshot: Option<ExternalSnapshotData>,
    provenance: Option<LinkProvenance>,
    allow_existing_link: bool,
) -> Result<Vec<Effect>, DomainError> {
    ensure_item(state, item_id)?;
    if object.canonical_url.trim().is_empty() {
        return Err(DomainError::EmptyExternalUrl);
    }
    if object.external_key.trim().is_empty() {
        return Err(DomainError::EmptyExternalObjectKey);
    }

    let existing_object = state
        .external_objects
        .iter()
        .find(|candidate| {
            candidate.provider == object.provider && candidate.external_key == object.external_key
        })
        .cloned();
    let (external_object, is_new_object) = match existing_object {
        Some(object) => (object, false),
        None => {
            let id = state.next_external_object_id;
            let next_external_object_id =
                id.checked_add(1).ok_or(DomainError::SequenceExhausted)?;
            let object = ExternalObject {
                id,
                provider: object.provider,
                kind: object.kind,
                external_key: object.external_key,
                canonical_url: object.canonical_url.trim().to_owned(),
            };
            state.next_external_object_id = next_external_object_id;
            state.external_objects.push(object.clone());
            (object, true)
        }
    };

    let mut effects = Vec::new();
    if is_new_object {
        effects.push(Effect::PersistExternalObject {
            object: external_object.clone(),
            next_external_object_id: state.next_external_object_id,
        });
    }

    if let Some(link) = state
        .links
        .iter_mut()
        .find(|link| link.item_id == item_id && link.external_object_id == external_object.id)
    {
        if !allow_existing_link {
            return Err(DomainError::LinkAlreadyExists);
        }
        if link.provenance != provenance {
            link.provenance = provenance;
            effects.push(Effect::PersistLinkState { link: link.clone() });
        }
    } else {
        let link_id = state.next_link_id;
        let next_link_id = link_id
            .checked_add(1)
            .ok_or(DomainError::SequenceExhausted)?;
        let reviewed_activity_id = state
            .activities
            .iter()
            .filter(|activity| activity.external_object_id == external_object.id)
            .map(|activity| activity.id)
            .max()
            .unwrap_or_default();
        let link = Link {
            id: link_id,
            item_id,
            external_object_id: external_object.id,
            reviewed_activity_id,
            attention_policy: None,
            watch_until: None,
            review_at: None,
            provenance,
        };
        state.next_link_id = next_link_id;
        state.links.push(link.clone());
        effects.push(Effect::PersistLink { link, next_link_id });
    }

    if let Some(snapshot_data) = snapshot {
        let snapshot = ExternalSnapshot {
            external_object_id: external_object.id,
            title: snapshot_data.title,
            state: snapshot_data.state,
            metadata: snapshot_data.metadata,
            fetched_at: snapshot_data.fetched_at,
        };
        let snapshot_changed = state
            .snapshots
            .iter()
            .find(|existing| existing.external_object_id == snapshot.external_object_id)
            != Some(&snapshot);
        if snapshot_changed {
            upsert_snapshot(state, snapshot.clone());
            effects.push(Effect::PersistExternalSnapshot { snapshot });
        }
    }

    Ok(effects)
}

fn effective_attention_policy(
    state: &DomainState,
    link: &Link,
    object: &ExternalObject,
) -> ExternalChangePolicy {
    if let Some(policy) = link.attention_policy {
        return policy;
    }
    let context_id = item_context_id(state, link.item_id).ok();
    context_id
        .and_then(|context_id| {
            state.attention_defaults.iter().find(|attention_default| {
                attention_default.context_id == context_id
                    && attention_default.object_kind == object.kind
            })
        })
        .map(|attention_default| attention_default.policy)
        .unwrap_or_else(ExternalChangePolicy::all)
}

fn attention_entry_for_link_at(
    state: &DomainState,
    link: &Link,
    object: &ExternalObject,
    now: Option<&str>,
) -> Option<AttentionEntry> {
    let policy = effective_attention_policy(state, link, object);
    let watch_active = now.is_none_or(|now| {
        link.watch_until
            .as_deref()
            .is_none_or(|watch_until| watch_until > now)
    });
    let activities = state
        .activities
        .iter()
        .filter(|activity| {
            watch_active
                && activity.external_object_id == object.id
                && activity.id > link.reviewed_activity_id
        })
        .filter_map(|activity| {
            let changes = activity
                .changes
                .iter()
                .filter(|change| policy.allows(change.kind))
                .cloned()
                .collect::<Vec<_>>();
            (!changes.is_empty()).then_some(Activity {
                id: activity.id,
                external_object_id: activity.external_object_id,
                observed_at: activity.observed_at,
                changes,
            })
        })
        .collect::<Vec<_>>();
    if activities.is_empty() {
        return None;
    }

    let source_title = state
        .snapshots
        .iter()
        .find(|snapshot| snapshot.external_object_id == object.id)
        .map(|snapshot| snapshot.title.clone())
        .unwrap_or_else(|| object.canonical_url.clone());
    let summary = activities
        .iter()
        .flat_map(|activity| activity.changes.iter())
        .map(format_change)
        .collect::<Vec<_>>()
        .join("; ");

    Some(AttentionEntry {
        kind: AttentionEntryKind::ExternalChange,
        link_id: link.id,
        reminder_id: None,
        run_id: None,
        item_id: link.item_id,
        external_object_id: object.id,
        source_title,
        source_url: object.canonical_url.clone(),
        activities,
        summary,
    })
}

fn snapshot_changes(
    previous: &ExternalSnapshot,
    current: &ExternalSnapshot,
) -> Vec<ExternalChange> {
    let mut changes = Vec::new();
    if previous.title != current.title {
        changes.push(ExternalChange {
            kind: ExternalChangeKind::Title,
            key: None,
            previous: Some(previous.title.clone()),
            current: Some(current.title.clone()),
        });
    }
    if previous.state != current.state {
        changes.push(ExternalChange {
            kind: ExternalChangeKind::State,
            key: None,
            previous: Some(previous.state.clone()),
            current: Some(current.state.clone()),
        });
    }

    let mut keys = previous
        .metadata
        .iter()
        .map(|metadata| metadata.key.clone())
        .chain(current.metadata.iter().map(|metadata| metadata.key.clone()))
        .collect::<Vec<_>>();
    keys.sort();
    keys.dedup();
    for key in keys {
        let previous_value = previous
            .metadata
            .iter()
            .find(|metadata| metadata.key == key)
            .map(|metadata| metadata.value.clone());
        let current_value = current
            .metadata
            .iter()
            .find(|metadata| metadata.key == key)
            .map(|metadata| metadata.value.clone());
        if previous_value != current_value {
            changes.push(ExternalChange {
                kind: ExternalChangeKind::Metadata,
                key: Some(key),
                previous: previous_value,
                current: current_value,
            });
        }
    }
    changes
}

fn format_change(change: &ExternalChange) -> String {
    let label = match change.kind {
        ExternalChangeKind::Title => "Title".to_owned(),
        ExternalChangeKind::State => "State".to_owned(),
        ExternalChangeKind::Metadata => {
            format!("Metadata {}", change.key.as_deref().unwrap_or("value"))
        }
    };
    match (&change.previous, &change.current) {
        (Some(previous), Some(current)) => format!("{label} changed from {previous} to {current}"),
        (None, Some(current)) => format!("{label} added as {current}"),
        (Some(previous), None) => format!("{label} removed (was {previous})"),
        (None, None) => format!("{label} changed"),
    }
}

#[cfg(test)]
mod workspace_contract_tests {
    use super::*;

    #[test]
    fn a_workspace_selects_repositories_and_worktree_execution_keeps_context() {
        let mut state = DomainState {
            next_context_id: 2,
            next_project_id: 2,
            next_item_id: 2,
            next_item_number: 2,
            next_repository_id: 2,
            next_workspace_id: 1,
            next_worktree_id: 1,
            next_machine_id: 2,
            next_run_id: 1,
            next_external_object_id: 1,
            next_link_id: 1,
            next_activity_id: 1,
            next_reminder_id: 1,
            contexts: vec![Context {
                id: 1,
                name: "Personal".into(),
                grill_defaults: GrillConfiguration::default(),
            }],
            projects: vec![Project {
                id: 1,
                context_id: 1,
                name: "Default".into(),
                defaults: ProjectDefaults::default(),
            }],
            repositories: vec![Repository {
                id: 1,
                project_id: 1,
                name: "mission-manager".into(),
                remote_url: "https://example.test/repo".into(),
                base_branch: "main".into(),
            }],
            repository_locations: Vec::new(),
            items: vec![Item {
                id: 1,
                human_identifier: "MC-1".into(),
                title: "Use Workspace vocabulary".into(),
                project_id: 1,
                status: ItemStatus::Active,
                notes: String::new(),
                reminders: Vec::new(),
            }],
            workspaces: Vec::new(),
            worktrees: Vec::new(),
            machines: vec![Machine {
                id: 1,
                context_id: 1,
                name: "Local".into(),
                socket_name: "mission".into(),
                transport: MachineTransport::Local,
                last_observed: MachineObservation::Available,
                last_observed_at: None,
            }],
            runs: Vec::new(),
            relationships: Vec::new(),
            external_objects: Vec::new(),
            links: Vec::new(),
            snapshots: Vec::new(),
            activities: Vec::new(),
            attention_defaults: Vec::new(),
        };

        let workspace = decide(
            state.clone(),
            Event::CreateWorkspace {
                item_id: 1,
                repositories: vec![WorkspaceRepositoryInput {
                    repository_id: 1,
                    branch: "feature/contract".into(),
                    base_branch: "main".into(),
                }],
            },
        )
        .expect("Workspace creation should succeed");
        state = workspace.state;
        assert_eq!(state.workspaces[0].item_id, 1);
        assert_eq!(state.workspaces[0].repositories[0].repository_id, 1);

        let worktree = decide(
            state,
            Event::CreateWorktree {
                workspace_id: 1,
                repository_id: 1,
                machine_id: 1,
                path: "/tmp/worktrees/mission-manager".into(),
                branch: "feature/contract".into(),
                base_branch: "main".into(),
                is_dirty: false,
            },
        )
        .expect("Worktree creation should succeed");
        assert_eq!(worktree.state.worktrees[0].workspace_id, 1);
        assert_eq!(
            worktree.state.workspaces[0].preparation_state,
            WorkspacePreparationState::Ready
        );
    }

    #[test]
    fn run_suggestions_only_attach_to_registered_workspace_locations() {
        let state = DomainState {
            next_context_id: 2,
            next_project_id: 2,
            next_item_id: 2,
            next_item_number: 2,
            next_repository_id: 2,
            next_workspace_id: 2,
            next_worktree_id: 1,
            next_machine_id: 2,
            next_run_id: 1,
            next_external_object_id: 1,
            next_link_id: 1,
            next_activity_id: 1,
            next_reminder_id: 1,
            contexts: vec![Context {
                id: 1,
                name: "Personal".into(),
                grill_defaults: GrillConfiguration::default(),
            }],
            projects: vec![Project {
                id: 1,
                context_id: 1,
                name: "Default".into(),
                defaults: ProjectDefaults::default(),
            }],
            repositories: vec![Repository {
                id: 1,
                project_id: 1,
                name: "repo".into(),
                remote_url: "https://example.test/repo".into(),
                base_branch: "main".into(),
            }],
            repository_locations: vec![RepositoryLocation {
                repository_id: 1,
                machine_id: 1,
                checkout_path: "/tmp/checkouts/repo".into(),
                worktree_root: "/tmp/worktrees".into(),
            }],
            items: vec![Item {
                id: 1,
                human_identifier: "MC-1".into(),
                title: "Attach".into(),
                project_id: 1,
                status: ItemStatus::Active,
                notes: String::new(),
                reminders: Vec::new(),
            }],
            workspaces: vec![Workspace {
                id: 1,
                item_id: 1,
                repositories: vec![WorkspaceRepository {
                    repository_id: 1,
                    branch: "main".into(),
                    base_branch: "main".into(),
                }],
                preparation_state: WorkspacePreparationState::Pending,
            }],
            worktrees: Vec::new(),
            machines: vec![Machine {
                id: 1,
                context_id: 1,
                name: "Local".into(),
                socket_name: "mission".into(),
                transport: MachineTransport::Local,
                last_observed: MachineObservation::Available,
                last_observed_at: None,
            }],
            runs: Vec::new(),
            relationships: Vec::new(),
            external_objects: Vec::new(),
            links: Vec::new(),
            snapshots: Vec::new(),
            activities: Vec::new(),
            attention_defaults: Vec::new(),
        };

        let suggestions = suggest_untracked_runs(
            &state,
            &[AgentPaneObservation {
                machine_id: 1,
                agent: AgentKind::Codex,
                session_name: "mission".into(),
                pane_id: "%1".into(),
                current_path: "/tmp/checkouts/repo/src".into(),
            }],
        );
        assert_eq!(suggestions.len(), 1);
        assert_eq!(suggestions[0].workspace_id, Some(1));
        assert_eq!(suggestions[0].repository_id, Some(1));
        assert_eq!(
            suggestions[0].location_path.as_deref(),
            Some("/tmp/checkouts/repo")
        );
    }

    #[test]
    fn home_relative_paths_do_not_match_unrelated_absolute_path_components() {
        assert!(path_is_within("~/src/repo", "~/src/repo/service"));
        let home = std::env::var("HOME").expect("the test environment should have a home");
        assert!(path_is_within(
            "~/src/repo",
            &format!("{home}/src/repo/service")
        ));
        assert!(!path_is_within("~/src/repo", "/tmp/src/repo/service"));
        assert!(!path_is_within(
            "/tmp/src/repo",
            "/tmp/src/repository/service"
        ));
    }
}

#[cfg(test)]
mod grill_contract_tests {
    use super::*;

    fn state() -> DomainState {
        DomainState {
            next_context_id: 2,
            next_project_id: 2,
            next_item_id: 2,
            next_item_number: 2,
            next_repository_id: 2,
            next_workspace_id: 2,
            next_worktree_id: 1,
            next_machine_id: 2,
            next_run_id: 1,
            next_external_object_id: 1,
            next_link_id: 1,
            next_activity_id: 1,
            next_reminder_id: 1,
            contexts: vec![Context {
                id: 1,
                name: "Personal".into(),
                grill_defaults: GrillConfiguration::default(),
            }],
            projects: vec![Project {
                id: 1,
                context_id: 1,
                name: "Default".into(),
                defaults: ProjectDefaults::default(),
            }],
            repositories: vec![Repository {
                id: 1,
                project_id: 1,
                name: "mission-manager".into(),
                remote_url: "https://example.test/repo".into(),
                base_branch: "main".into(),
            }],
            repository_locations: vec![RepositoryLocation {
                repository_id: 1,
                machine_id: 1,
                checkout_path: "/tmp/mission-manager".into(),
                worktree_root: "/tmp/worktrees".into(),
            }],
            items: vec![Item {
                id: 1,
                human_identifier: "MC-1".into(),
                title: "Decide the next architecture".into(),
                project_id: 1,
                status: ItemStatus::Active,
                notes: "The decision must stay reversible.".into(),
                reminders: Vec::new(),
            }],
            workspaces: vec![Workspace {
                id: 1,
                item_id: 1,
                repositories: vec![WorkspaceRepository {
                    repository_id: 1,
                    branch: "main".into(),
                    base_branch: "main".into(),
                }],
                preparation_state: WorkspacePreparationState::Ready,
            }],
            worktrees: Vec::new(),
            machines: vec![Machine {
                id: 1,
                context_id: 1,
                name: "Local".into(),
                socket_name: "mission".into(),
                transport: MachineTransport::Local,
                last_observed: MachineObservation::Available,
                last_observed_at: None,
            }],
            runs: Vec::new(),
            relationships: Vec::new(),
            external_objects: Vec::new(),
            links: Vec::new(),
            snapshots: Vec::new(),
            activities: Vec::new(),
            attention_defaults: Vec::new(),
        }
    }

    fn start_event(configuration: GrillConfiguration) -> Event {
        let prompt = compose_grill_prompt(
            &state(),
            1,
            &configuration,
            "Stress-test this architecture decision.",
        )
        .expect("the Grill prompt should be composable");
        Event::StartGrillRun {
            item_id: 1,
            workspace_id: 1,
            repository_id: 1,
            machine_id: 1,
            configuration,
            prompt,
            skill_snapshot: GRILL_SKILL_SNAPSHOT.into(),
            working_directory: "/tmp/mission-manager".into(),
            session_name: "mission-item-1-run-1".into(),
            pane_id: "%1".into(),
            started_at: 123,
            checkouts: vec![RunCheckout {
                repository_id: 1,
                path: "/tmp/mission-manager".into(),
                branch: "main".into(),
                is_dirty: false,
            }],
        }
    }

    #[test]
    fn grill_catalog_rejects_an_effort_not_supported_by_the_selected_model() {
        let configuration = GrillConfiguration {
            agent: AgentKind::Codex,
            model: "codex-luna".into(),
            effort: "not-supported".into(),
        };

        assert!(validate_grill_configuration(&configuration).is_err());
        assert!(grill_model_catalog()
            .iter()
            .any(|catalog| catalog.agent == AgentKind::Codex
                && catalog.models.iter().any(|model| {
                    model.id == "codex-luna"
                        && model.efforts.iter().any(|effort| effort.id == "xhigh")
                })));
    }

    #[test]
    fn a_grill_run_persists_its_configuration_snapshot_and_primary_checkout() {
        let configuration = GrillConfiguration {
            agent: AgentKind::Codex,
            model: "codex-luna".into(),
            effort: "xhigh".into(),
        };

        let decision = decide(state(), start_event(configuration.clone()))
            .expect("a valid Grill should start");
        let run = decision.state.runs.last().expect("Run should be recorded");

        assert_eq!(run.execution_profile, ExecutionProfile::Grill);
        assert_eq!(run.repository_id, Some(1));
        assert_eq!(run.working_directory, "/tmp/mission-manager");
        assert_eq!(run.model.as_deref(), Some("codex-luna"));
        assert_eq!(run.effort.as_deref(), Some("xhigh"));
        assert_eq!(run.skill_snapshot.as_deref(), Some(GRILL_SKILL_SNAPSHOT));
        assert!(run
            .prompt
            .contains("Stress-test this architecture decision."));
        assert!(run.prompt.contains("Decide the next architecture"));
        assert!(run.prompt.contains("The decision must stay reversible."));
        assert!(run.prompt.contains(GRILL_SKILL_SNAPSHOT));
    }

    #[test]
    fn an_item_cannot_start_a_second_active_grill_run() {
        let configuration = GrillConfiguration::default();
        let first = decide(state(), start_event(configuration.clone()))
            .expect("the first Grill should start");
        let error = decide(first.state, start_event(configuration))
            .expect_err("a second active Grill must be rejected");

        assert!(matches!(
            error,
            DomainError::ActiveGrillRun { item_id: 1, .. }
        ));
    }

    #[test]
    fn parser_extracts_recommendations_options_and_free_form_questions() {
        let group = parse_grill_question_group(
            "before the group\n\n❓ **Q1** - **Repository layout**: Which layout should we keep?\n➡️ **Keep the current layout**\nA) Keep current\nB. Split repositories\n---\n❓ 2. What should we document next?\n",
        )
        .expect("the question group should parse");

        assert_eq!(group.questions.len(), 2);
        assert_eq!(group.questions[0].number, 1);
        assert_eq!(
            group.questions[0].title.as_deref(),
            Some("Repository layout")
        );
        assert_eq!(
            group.questions[0].recommendation.as_deref(),
            Some("Keep the current layout")
        );
        assert_eq!(
            group.questions[0].options,
            vec![
                GrillOption {
                    key: "A".into(),
                    label: "Keep current".into(),
                },
                GrillOption {
                    key: "B".into(),
                    label: "Split repositories".into(),
                },
            ]
        );
        assert_eq!(group.questions[1].number, 2);
        assert!(group.questions[1].options.is_empty());
        assert_eq!(group.questions[1].prompt, "What should we document next?");
    }

    #[test]
    fn malformed_grill_text_does_not_become_a_question_group() {
        assert!(parse_grill_question_group("❓\n➡️\n---").is_none());
        assert!(parse_grill_question_group("The agent is still working").is_none());
    }

    #[test]
    fn parsing_a_later_grill_response_ignores_questions_from_the_retained_scrollback() {
        let first_transcript = "❓ Q1: Which layout should we keep?\nA) Current\n";
        let transcript =
            format!("{first_transcript}1. Current\n\n❓ Q1: Confirm the specification?\n➡️ Yes\n");

        let group = parse_grill_question_group_since(first_transcript, &transcript)
            .expect("the later response should contain a question group");

        assert_eq!(group.questions.len(), 1);
        assert_eq!(group.questions[0].prompt, "Confirm the specification?");
    }

    #[test]
    fn an_unparseable_later_grill_response_clears_the_previous_pending_group() {
        let started = decide(state(), start_event(GrillConfiguration::default()))
            .expect("the Grill should start");
        let first_group = parse_grill_question_group("❓ Q1: Which layout should we keep?")
            .expect("the first response should parse");
        let waiting = decide(
            started.state,
            Event::RecordRunTranscript {
                run_id: 1,
                transcript: "first response".into(),
                question_group: Some(first_group),
            },
        )
        .expect("the first response should be retained");
        let pane_lost = decide(
            waiting.state,
            Event::SetRunPaneStatus {
                run_id: 1,
                status: RunPaneStatus::Missing,
            },
        )
        .expect("a missing Pane should preserve the waiting Run");
        let reconnected = decide(
            pane_lost.state,
            Event::SetRunPaneStatus {
                run_id: 1,
                status: RunPaneStatus::Available,
            },
        )
        .expect("the exact Pane should be recoverable");
        let answered = decide(
            reconnected.state,
            Event::RecordGrillAnswers {
                run_id: 1,
                answers: vec![GrillAnswer {
                    question_number: 1,
                    answer: "Keep current".into(),
                }],
            },
        )
        .expect("the first answer should be retained");
        let responded = decide(
            answered.state,
            Event::RecordGrillResponse {
                run_id: 1,
                response: "1. Keep current".into(),
            },
        )
        .expect("the first response should be retained");

        let recaptured = decide(
            responded.state,
            Event::RecordRunTranscript {
                run_id: 1,
                transcript: "a later response with no parseable question markers".into(),
                question_group: None,
            },
        )
        .expect("an unparseable response should still be retained");
        let run = &recaptured.state.runs[0];

        assert!(run.grill_question_group.is_none());
        assert!(run.grill_answers.is_empty());
        assert!(run.grill_response.is_none());
        assert_eq!(
            run.transcript,
            "a later response with no parseable question markers"
        );
    }

    #[test]
    fn one_grill_answer_event_persists_a_stable_numbered_response() {
        let started = decide(state(), start_event(GrillConfiguration::default()))
            .expect("the Grill should start");
        let group = parse_grill_question_group(
            "❓ Q1: Which option?\n➡️ Use A\nA) Use A\nB) Use B\n❓ Q2: Explain the tradeoff",
        )
        .expect("the group should parse");
        let recorded = decide(
            started.state,
            Event::RecordRunTranscript {
                run_id: 1,
                transcript: "raw response".into(),
                question_group: Some(group),
            },
        )
        .expect("the transcript should be recorded");
        let answered = decide(
            recorded.state,
            Event::RecordGrillAnswers {
                run_id: 1,
                answers: vec![
                    GrillAnswer {
                        question_number: 2,
                        answer: "Document the tradeoff".into(),
                    },
                    GrillAnswer {
                        question_number: 1,
                        answer: "Recommendation: Use A".into(),
                    },
                ],
            },
        )
        .expect("the answers should be recorded");
        let response = format_grill_response(&answered.state.runs[0].grill_answers)
            .expect("the grouped response should format");
        let completed = decide(
            answered.state,
            Event::RecordGrillResponse {
                run_id: 1,
                response,
            },
        )
        .expect("the grouped response should be recorded");

        assert_eq!(completed.state.runs[0].transcript, "raw response");
        assert_eq!(
            completed.state.runs[0].grill_response.as_deref(),
            Some("1. Recommendation: Use A\n2. Document the tradeoff")
        );
    }

    #[test]
    fn a_grill_run_reconciles_hook_state_and_pane_recovery_without_text_inference() {
        let started = decide(state(), start_event(GrillConfiguration::default()))
            .expect("the Grill should start");
        assert_eq!(
            started.state.runs[0].grill_phase,
            Some(GrillPhase::Starting)
        );

        let working = decide(
            started.state,
            Event::UpdateRunState {
                run_id: 1,
                state: RunState::Working,
            },
        )
        .expect("the hook state should be recorded");
        assert_eq!(working.state.runs[0].grill_phase, Some(GrillPhase::Working));

        let group =
            parse_grill_question_group("❓ Q1: Which option?").expect("the group should parse");
        let transcript = decide(
            working.state,
            Event::RecordRunTranscript {
                run_id: 1,
                transcript: "agent finished a response".into(),
                question_group: Some(group.clone()),
            },
        )
        .expect("transcript capture should be recorded");
        assert_eq!(
            transcript.state.runs[0].grill_phase,
            Some(GrillPhase::Working)
        );

        let answered = decide(
            transcript.state,
            Event::RecordGrillAnswers {
                run_id: 1,
                answers: vec![GrillAnswer {
                    question_number: 1,
                    answer: "Keep it".into(),
                }],
            },
        )
        .expect("the answer should be recorded");
        let responded = decide(
            answered.state,
            Event::RecordGrillResponse {
                run_id: 1,
                response: "1. Keep it".into(),
            },
        )
        .expect("the response should be recorded");

        let recaptured = decide(
            responded.state,
            Event::RecordRunTranscript {
                run_id: 1,
                transcript: "latest transcript with the same question".into(),
                question_group: Some(group),
            },
        )
        .expect("a later capture should be recorded");
        assert_eq!(recaptured.state.runs[0].grill_answers.len(), 1);
        assert_eq!(
            recaptured.state.runs[0].grill_response.as_deref(),
            Some("1. Keep it")
        );

        let blocked = decide(
            recaptured.state,
            Event::UpdateRunState {
                run_id: 1,
                state: RunState::Blocked,
            },
        )
        .expect("the hook should report a waiting state");
        assert_eq!(
            blocked.state.runs[0].grill_phase,
            Some(GrillPhase::WaitingForAnswers)
        );

        let missing = decide(
            blocked.state,
            Event::SetRunPaneStatus {
                run_id: 1,
                status: RunPaneStatus::Missing,
            },
        )
        .expect("Pane loss should remain recoverable");
        let missing_run = &missing.state.runs[0];
        assert_eq!(missing_run.state, RunState::Blocked);
        assert_eq!(missing_run.pane_status, RunPaneStatus::Missing);
        assert_eq!(
            missing_run.grill_phase,
            Some(GrillPhase::RecoverablePaneLoss)
        );
        assert_eq!(missing_run.grill_answers.len(), 1);
        assert_eq!(
            missing_run.transcript,
            "latest transcript with the same question"
        );

        let reconnected = decide(
            missing.state,
            Event::SetRunPaneStatus {
                run_id: 1,
                status: RunPaneStatus::Available,
            },
        )
        .expect("Pane recovery should restore the specialized phase");
        assert_eq!(
            reconnected.state.runs[0].grill_phase,
            Some(GrillPhase::WaitingForAnswers)
        );
        assert_eq!(reconnected.state.items[0].status, ItemStatus::Active);
    }

    #[test]
    fn finishing_a_grill_run_is_explicit_and_keeps_the_item_status_unchanged() {
        let started = decide(state(), start_event(GrillConfiguration::default()))
            .expect("the Grill should start");
        let finished = decide(started.state, Event::FinishRun { run_id: 1 })
            .expect("the user should be able to finish the Run explicitly");
        assert_eq!(finished.state.runs[0].state, RunState::Finished);
        assert_eq!(
            finished.state.runs[0].grill_phase,
            Some(GrillPhase::Finished)
        );
        assert_eq!(finished.state.items[0].status, ItemStatus::Active);
    }

    #[test]
    fn a_completed_grill_stays_active_until_the_user_finishes_it() {
        let started = decide(state(), start_event(GrillConfiguration::default()))
            .expect("the Grill should start");
        let awaiting = decide(
            started.state,
            Event::UpdateRunState {
                run_id: 1,
                state: RunState::Finished,
            },
        )
        .expect("the hook completion should enter the downstream phase");

        assert_eq!(awaiting.state.runs[0].state, RunState::Finished);
        assert_eq!(
            awaiting.state.runs[0].grill_phase,
            Some(GrillPhase::AwaitingNextAction)
        );
        let error = decide(awaiting.state, start_event(GrillConfiguration::default()))
            .expect_err("an awaiting Grill still occupies the Item");
        assert!(matches!(
            error,
            DomainError::ActiveGrillRun { item_id: 1, .. }
        ));
    }

    #[test]
    fn downstream_prompt_contains_the_selected_skill_item_context_transcript_and_decisions() {
        let started = decide(state(), start_event(GrillConfiguration::default()))
            .expect("the Grill should start");
        let transcript = decide(
            started.state,
            Event::RecordRunTranscript {
                run_id: 1,
                transcript: "❓ Q1: Which decision should we keep?".into(),
                question_group: parse_grill_question_group("❓ Q1: Which decision should we keep?"),
            },
        )
        .expect("the Grill transcript should be recorded");
        let answered = decide(
            transcript.state,
            Event::RecordGrillAnswers {
                run_id: 1,
                answers: vec![GrillAnswer {
                    question_number: 1,
                    answer: "Keep the reversible design".into(),
                }],
            },
        )
        .expect("the decision should be recorded");
        let responded = decide(
            answered.state,
            Event::RecordGrillResponse {
                run_id: 1,
                response: "1. Keep the reversible design".into(),
            },
        )
        .expect("the grouped response should be recorded");
        let awaiting = decide(
            responded.state,
            Event::UpdateRunState {
                run_id: 1,
                state: RunState::Finished,
            },
        )
        .expect("the Grill should await a downstream action");

        for action in [
            GrillContinuationAction::ToSpec,
            GrillContinuationAction::ToTickets,
            GrillContinuationAction::Implement,
        ] {
            let prompt = compose_grill_continuation_prompt(&awaiting.state, 1, action)
                .expect("the downstream prompt should compose");
            assert!(prompt.contains(action.skill_snapshot()));
            assert!(prompt.contains("Decide the next architecture"));
            assert!(prompt.contains("The decision must stay reversible."));
            assert!(prompt.contains("❓ Q1: Which decision should we keep?"));
            assert!(prompt.contains("Q1: Keep the reversible design"));
            assert!(prompt.contains(action.as_str()));
            assert!(prompt.contains("same Run and Pane"));
            assert!(prompt.contains("AI_MISSION_MANAGER_EVENT"));
            assert!(prompt.contains("github.issue.created"));
            assert!(prompt.contains("\"run_id\":1"));
        }
    }

    #[test]
    fn selecting_a_downstream_skill_reuses_the_run_and_allows_grouped_questions_again() {
        let started = decide(state(), start_event(GrillConfiguration::default()))
            .expect("the Grill should start");
        let awaiting = decide(
            started.state,
            Event::UpdateRunState {
                run_id: 1,
                state: RunState::Finished,
            },
        )
        .expect("the Grill should await a downstream action");
        let continued = decide(
            awaiting.state,
            Event::ContinueGrill {
                run_id: 1,
                action: GrillContinuationAction::ToSpec,
            },
        )
        .expect("to-spec should continue the existing Run");
        assert_eq!(continued.state.runs[0].id, 1);
        assert_eq!(continued.state.runs[0].state, RunState::Working);
        assert_eq!(
            continued.state.runs[0].grill_phase,
            Some(GrillPhase::Working)
        );
        assert_eq!(continued.state.runs[0].pane_id, "%1");
        assert_eq!(
            continued.state.runs[0].working_directory,
            "/tmp/mission-manager"
        );
        assert!(continued.state.runs[0].grill_question_group.is_none());
        assert!(continued.state.runs[0].grill_response.is_none());

        let group = parse_grill_question_group("❓ Q1: Confirm the specification?")
            .expect("the downstream skill should use the same parser");
        let waiting = decide(
            continued.state,
            Event::UpdateRunState {
                run_id: 1,
                state: RunState::Blocked,
            },
        )
        .expect("the downstream hook should report a question");
        let recorded = decide(
            waiting.state,
            Event::RecordRunTranscript {
                run_id: 1,
                transcript: "downstream confirmation".into(),
                question_group: Some(group),
            },
        )
        .expect("the downstream question group should be recorded");
        assert_eq!(
            recorded.state.runs[0].grill_phase,
            Some(GrillPhase::WaitingForAnswers)
        );
        assert_eq!(recorded.state.runs[0].session_name, "mission-item-1-run-1");
    }

    #[test]
    fn the_work_projection_keeps_one_grill_run_through_answers_recovery_and_both_downstream_skills()
    {
        let started = decide(state(), start_event(GrillConfiguration::default()))
            .expect("the Grill should start");
        let working = decide(
            started.state,
            Event::UpdateRunState {
                run_id: 1,
                state: RunState::Working,
            },
        )
        .expect("the hook should report the active Grill");
        let first_group = parse_grill_question_group("❓ Q1: Which decision should we keep?")
            .expect("the first question group should parse");
        let transcript = decide(
            working.state,
            Event::RecordRunTranscript {
                run_id: 1,
                transcript: "initial Grill response\n❓ Q1: Which decision should we keep?".into(),
                question_group: Some(first_group),
            },
        )
        .expect("the initial transcript should be retained");
        let waiting = decide(
            transcript.state,
            Event::UpdateRunState {
                run_id: 1,
                state: RunState::Blocked,
            },
        )
        .expect("the hook should put the Run into the waiting phase");
        let answered = decide(
            waiting.state,
            Event::RecordGrillAnswers {
                run_id: 1,
                answers: vec![GrillAnswer {
                    question_number: 1,
                    answer: "Keep the reversible design".into(),
                }],
            },
        )
        .expect("the grouped answer should be persisted");
        let responded = decide(
            answered.state,
            Event::RecordGrillResponse {
                run_id: 1,
                response: "1. Keep the reversible design".into(),
            },
        )
        .expect("the grouped answer should be recorded as a response");
        let grill_finished = decide(
            responded.state,
            Event::UpdateRunState {
                run_id: 1,
                state: RunState::Finished,
            },
        )
        .expect("the completed Grill should await a downstream action");
        assert_eq!(
            grill_finished.state.runs[0].grill_phase,
            Some(GrillPhase::AwaitingNextAction)
        );

        let to_spec = decide(
            grill_finished.state,
            Event::ContinueGrill {
                run_id: 1,
                action: GrillContinuationAction::ToSpec,
            },
        )
        .expect("to-spec should reuse the Run");
        let spec_finished = decide(
            to_spec.state,
            Event::UpdateRunState {
                run_id: 1,
                state: RunState::Finished,
            },
        )
        .expect("to-spec should return to the downstream action frontier");
        let spec_issue = confirmed_downstream_issue("https://github.com/acme/app/issues/7");
        let spec_captured = decide(
            spec_finished.state,
            Event::CaptureDownstreamIssues {
                run_id: 1,
                action: GrillContinuationAction::ToSpec,
                issues: vec![spec_issue],
            },
        )
        .expect("to-spec output should link its confirmed Issue");

        let to_tickets = decide(
            spec_captured.state,
            Event::ContinueGrill {
                run_id: 1,
                action: GrillContinuationAction::ToTickets,
            },
        )
        .expect("to-tickets should reuse the same Run");
        let tickets_finished = decide(
            to_tickets.state,
            Event::UpdateRunState {
                run_id: 1,
                state: RunState::Finished,
            },
        )
        .expect("to-tickets should return to the downstream action frontier");
        let tickets_issue = confirmed_downstream_issue("https://github.com/acme/app/issues/8");
        let captured = decide(
            tickets_finished.state,
            Event::CaptureDownstreamIssues {
                run_id: 1,
                action: GrillContinuationAction::ToTickets,
                issues: vec![tickets_issue],
            },
        )
        .expect("to-tickets output should link its confirmed Issue");

        let view = home_view(&captured.state, None, "2026-09-22T12:00");
        let item = view
            .running
            .iter()
            .find(|item| item.item.id == 1)
            .expect("the Item should remain in its original Work column");
        assert_eq!(item.item.status, ItemStatus::Active);
        assert_eq!(item.runs.len(), 1);
        assert_eq!(item.runs[0].id, 1);
        assert_eq!(item.runs[0].pane_id, "%1");
        assert_eq!(item.runs[0].working_directory, "/tmp/mission-manager");
        assert_eq!(item.links.len(), 2);
        assert_eq!(
            item.links
                .iter()
                .map(|link| link
                    .link
                    .provenance
                    .as_ref()
                    .map(|provenance| provenance.action))
                .collect::<Vec<_>>(),
            vec![
                Some(GrillContinuationAction::ToSpec),
                Some(GrillContinuationAction::ToTickets)
            ]
        );

        let finished = decide(captured.state, Event::FinishRun { run_id: 1 })
            .expect("finishing the Run should be explicit");
        assert_eq!(finished.state.items[0].status, ItemStatus::Active);
        assert_eq!(
            finished.state.runs[0].grill_phase,
            Some(GrillPhase::Finished)
        );
    }

    #[test]
    fn downstream_action_requires_the_completed_grill_and_an_available_pane() {
        let started = decide(state(), start_event(GrillConfiguration::default()))
            .expect("the Grill should start");
        let error = decide(
            started.state,
            Event::ContinueGrill {
                run_id: 1,
                action: GrillContinuationAction::Implement,
            },
        )
        .expect_err("a Grill still asking questions cannot jump downstream");
        assert!(matches!(
            error,
            DomainError::GrillContinuationNotAvailable { run_id: 1, .. }
        ));

        let awaiting = decide(
            decide(state(), start_event(GrillConfiguration::default()))
                .expect("the Grill should start")
                .state,
            Event::UpdateRunState {
                run_id: 1,
                state: RunState::Finished,
            },
        )
        .expect("the Grill should await a downstream action");
        let missing = decide(
            awaiting.state,
            Event::SetRunPaneStatus {
                run_id: 1,
                status: RunPaneStatus::Missing,
            },
        )
        .expect("Pane loss should be recoverable");
        let error = decide(
            missing.state,
            Event::ContinueGrill {
                run_id: 1,
                action: GrillContinuationAction::Implement,
            },
        )
        .expect_err("a missing Pane cannot receive a continuation");
        assert!(matches!(
            error,
            DomainError::GrillContinuationNotAvailable { run_id: 1, .. }
        ));
    }

    #[test]
    fn downstream_issue_discovery_prefers_structured_events_and_falls_back_to_issue_urls() {
        let structured = discover_downstream_issue_candidates(
            r#"AI_MISSION_MANAGER_EVENT {"event":"github.issue.created","url":"https://github.com/acme/app/issues/7","run_id":1,"action":"to-tickets"}
https://github.com/acme/app/issues/8
https://example.com/unrelated"#,
        );
        assert_eq!(structured.len(), 1);
        assert_eq!(structured[0].url, "https://github.com/acme/app/issues/7");
        assert_eq!(
            structured[0].discovery,
            DownstreamIssueDiscovery::StructuredEvent
        );
        assert_eq!(structured[0].run_id, Some(1));
        assert_eq!(
            structured[0].action,
            Some(GrillContinuationAction::ToTickets)
        );

        let fallback = discover_downstream_issue_candidates(
            "Created issues: https://github.com/acme/app/issues/8/ and https://github.com/acme/app/issues/8/).",
        );
        assert_eq!(fallback.len(), 1);
        assert_eq!(fallback[0].url, "https://github.com/acme/app/issues/8");
        assert_eq!(fallback[0].discovery, DownstreamIssueDiscovery::OutputUrl);
    }

    #[test]
    fn captured_downstream_issue_is_idempotent_and_keeps_run_action_provenance() {
        let started = decide(state(), start_event(GrillConfiguration::default()))
            .expect("the Grill should start");
        let awaiting = decide(
            started.state,
            Event::UpdateRunState {
                run_id: 1,
                state: RunState::Finished,
            },
        )
        .expect("the Grill should finish");
        let continued = decide(
            awaiting.state,
            Event::ContinueGrill {
                run_id: 1,
                action: GrillContinuationAction::ToTickets,
            },
        )
        .expect("to-tickets should continue the Run");
        let issue = confirmed_downstream_issue("https://github.com/acme/app/issues/7");
        let second_item = decide(
            continued.state,
            Event::CreateItem {
                title: "Another Item".into(),
                context_id: 1,
                project_id: 1,
            },
        )
        .expect("a second Item should be created");
        let linked_elsewhere = decide(
            second_item.state,
            Event::LinkExternalObject {
                item_id: 2,
                object: issue.object.clone(),
                snapshot: Some(issue.snapshot.clone()),
            },
        )
        .expect("the same External Object should be linkable to another Item");

        let first = decide(
            linked_elsewhere.state,
            Event::CaptureDownstreamIssues {
                run_id: 1,
                action: GrillContinuationAction::ToTickets,
                issues: vec![issue.clone()],
            },
        )
        .expect("the confirmed Issue should be captured");
        assert_eq!(first.state.external_objects.len(), 1);
        assert_eq!(first.state.links.len(), 2);
        assert_eq!(
            first
                .state
                .links
                .iter()
                .find(|link| link.item_id == 1)
                .expect("the captured Item Link should exist")
                .provenance,
            Some(LinkProvenance {
                run_id: 1,
                action: GrillContinuationAction::ToTickets,
                discovery: DownstreamIssueDiscovery::StructuredEvent,
            })
        );

        let second = decide(
            first.state,
            Event::CaptureDownstreamIssues {
                run_id: 1,
                action: GrillContinuationAction::ToTickets,
                issues: vec![issue],
            },
        )
        .expect("capturing the same Issue again should be safe");
        assert_eq!(second.state.external_objects.len(), 1);
        assert_eq!(second.state.links.len(), 2);
        assert!(second.effects.is_empty());
    }

    #[test]
    fn deleting_a_captured_link_only_removes_local_external_state() {
        let started = decide(state(), start_event(GrillConfiguration::default()))
            .expect("the Grill should start");
        let awaiting = decide(
            started.state,
            Event::UpdateRunState {
                run_id: 1,
                state: RunState::Finished,
            },
        )
        .expect("the Grill should finish");
        let continued = decide(
            awaiting.state,
            Event::ContinueGrill {
                run_id: 1,
                action: GrillContinuationAction::ToTickets,
            },
        )
        .expect("to-tickets should continue the Run");
        let captured = decide(
            continued.state,
            Event::CaptureDownstreamIssues {
                run_id: 1,
                action: GrillContinuationAction::ToTickets,
                issues: vec![confirmed_downstream_issue(
                    "https://github.com/acme/app/issues/7",
                )],
            },
        )
        .expect("the Issue should be captured");

        let deleted = decide(captured.state, Event::DeleteLink { link_id: 1 })
            .expect("deleting the local Link should succeed");
        assert!(deleted.state.links.is_empty());
        assert!(deleted.state.external_objects.is_empty());
        assert!(deleted.effects.iter().all(|effect| matches!(
            effect,
            Effect::RemoveLink { .. } | Effect::RemoveExternalObject { .. }
        )));
    }

    fn confirmed_downstream_issue(url: &str) -> ConfirmedDownstreamIssue {
        let object = crate::provider::classify_url(url).expect("fixture URL should classify");
        ConfirmedDownstreamIssue {
            object,
            snapshot: ExternalSnapshotData {
                title: "Captured Issue".into(),
                state: "OPEN".into(),
                metadata: vec![],
                fetched_at: 123,
            },
            discovery: DownstreamIssueDiscovery::StructuredEvent,
        }
    }
}
