use serde::{Deserialize, Serialize};

use super::*;

pub const GRILL_SKILL_SNAPSHOT: &str = include_str!("../../../.agents/skills/grilling/SKILL.md");
pub const TO_SPEC_SKILL_SNAPSHOT: &str = include_str!("../../../.agents/skills/to-spec/SKILL.md");
pub const TO_TICKETS_SKILL_SNAPSHOT: &str =
    include_str!("../../../.agents/skills/to-tickets/SKILL.md");
pub const IMPLEMENT_SKILL_SNAPSHOT: &str =
    include_str!("../../../.agents/skills/implement/SKILL.md");

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
    parse_grill_question_group_with_fence_state(&transcript, None)
}

fn parse_grill_question_group_with_fence_state(
    transcript: &str,
    mut code_fence: Option<(char, usize)>,
) -> Option<GrillQuestionGroup> {
    let mut questions = Vec::new();
    let mut current: Option<GrillQuestion> = None;

    for line in transcript.lines() {
        if skip_markdown_code_fence_line(line, &mut code_fence) {
            continue;
        }
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
    if let Some(response) = transcript.strip_prefix(previous_transcript.as_str()) {
        let code_fence = markdown_code_fence_state(&previous_transcript);
        parse_grill_question_group_with_fence_state(response, code_fence)
    } else {
        parse_grill_question_group_with_fence_state(&transcript, None)
    }
}

fn markdown_code_fence_state(transcript: &str) -> Option<(char, usize)> {
    let mut code_fence = None;
    for line in transcript.lines() {
        skip_markdown_code_fence_line(line, &mut code_fence);
    }
    code_fence
}

fn skip_markdown_code_fence_line(
    line: &str,
    code_fence: &mut Option<(char, usize)>,
) -> bool {
    let trimmed = line.trim_start();
    let mut characters = trimmed.chars();
    let Some(marker @ ('`' | '~')) = characters.next() else {
        return code_fence.is_some();
    };
    let marker_length = 1 + characters
        .take_while(|character| *character == marker)
        .count();
    if marker_length < 3 {
        return code_fence.is_some();
    }

    let fence_tail = &trimmed[marker_length..];
    match *code_fence {
        Some((open_marker, open_length))
            if marker == open_marker
                && marker_length >= open_length
                && fence_tail.trim().is_empty() =>
        {
            *code_fence = None;
        }
        Some(_) => {}
        None => *code_fence = Some((marker, marker_length)),
    }
    true
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
                    id: "claude-opus-5".into(),
                    label: "Opus 5".into(),
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
                    id: "gpt-6-sol".into(),
                    label: "GPT-6 Sol".into(),
                    efforts: efforts(&[
                        ("low", "Low"),
                        ("medium", "Medium"),
                        ("high", "High"),
                        ("xhigh", "Extra high"),
                    ]),
                },
                GrillModel {
                    id: "gpt-6-luna".into(),
                    label: "GPT-6 Luna".into(),
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
