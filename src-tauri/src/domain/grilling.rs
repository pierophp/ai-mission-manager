use serde::{Deserialize, Serialize};

use super::*;

pub const GRILL_SKILL_SNAPSHOT: &str = include_str!("../../../.agents/skills/grilling/SKILL.md");
pub const TO_SPEC_SKILL_SNAPSHOT: &str = include_str!("../../../.agents/skills/to-spec/SKILL.md");
pub const TO_TICKETS_SKILL_SNAPSHOT: &str =
    include_str!("../../../.agents/skills/to-tickets/SKILL.md");
pub const IMPLEMENT_SKILL_SNAPSHOT: &str =
    include_str!("../../../.agents/skills/implement/SKILL.md");

/// How Mission Manager reads questions out of the Pane. Agents render
/// Markdown differently (Codex drops bold, rewrites `---`, wraps lines), so the
/// contract only relies on line-leading markers.
pub const GRILL_OUTPUT_CONTRACT: &str = "Mission Manager output contract (it reads your questions from the terminal and shows them to the user as a form):
- Start every question on its own line with `❓ **Q<n>** - **<title>**: <question>`. Number questions 1, 2, 3… within the round.
- Put the recommendation on the line that starts with `➡️`. Do not add prose after the recommendation other than lettered options (`A) …`).
- Separate questions with a line containing only `---`.
- Print each round exactly once, at the end of your turn. If a sub-agent you are waiting on changes a question, print only the revised full round; Mission Manager shows only the last round printed in a turn.
- Keep status notes (what you are checking, what you found) before the first `❓`, never between or after the questions.
- The user answers every question of the round at once, with one numbered reply (`1. …`, `2. …`). An answer of `ok` accepts your recommendation.";

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

/// A downstream action normally follows a finished Grill. to-spec may also cut
/// a Grill short while it waits for answers: the agent is idle, and the open
/// questions go into the spec instead of being answered.
pub fn grill_continuation_available(
    phase: Option<GrillPhase>,
    action: GrillContinuationAction,
) -> bool {
    match phase {
        Some(GrillPhase::AwaitingNextAction) => true,
        Some(GrillPhase::WaitingForAnswers) => action == GrillContinuationAction::ToSpec,
        _ => false,
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
    // Structured events come first so their Run and action provenance wins,
    // but plain URLs still count: a wrapped or forgotten event line must not
    // hide an Issue. Only Issues created after the action started are kept.
    let urls = output.split_whitespace().filter_map(|token| {
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
    });
    unique_issue_candidates(structured.into_iter().chain(urls).collect())
}

/// Clock skew allowed between this Mac and GitHub when comparing an Issue's
/// creation time with the moment a downstream action was sent.
const DOWNSTREAM_CLOCK_SKEW_SECONDS: i64 = 120;

/// Whether a confirmed Issue was created by the current downstream action,
/// rather than merely mentioned in the transcript (the Item's own source, an
/// Issue the agent read, a spec created by an earlier action).
pub fn downstream_issue_is_new(
    snapshot: &ExternalSnapshotData,
    action_started_at: Option<i64>,
) -> bool {
    let Some(started_at) = action_started_at else {
        return true;
    };
    snapshot
        .metadata
        .iter()
        .find(|metadata| metadata.key == "created")
        .and_then(|metadata| parse_github_timestamp(&metadata.value))
        .is_some_and(|created_at| created_at + DOWNSTREAM_CLOCK_SKEW_SECONDS >= started_at)
}

/// Parse GitHub's `YYYY-MM-DDTHH:MM:SSZ` timestamps into Unix seconds.
pub fn parse_github_timestamp(value: &str) -> Option<i64> {
    let value = value.trim().strip_suffix('Z')?;
    let (date, time) = value.split_once('T')?;
    let mut date = date.splitn(3, '-').map(|part| part.parse::<i64>().ok());
    let (year, month, day) = (date.next()??, date.next()??, date.next()??);
    let time = time.split('.').next()?;
    let mut time = time.splitn(3, ':').map(|part| part.parse::<i64>().ok());
    let (hour, minute, second) = (time.next()??, time.next()??, time.next()??);
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    // Days from civil, proleptic Gregorian (Howard Hinnant's algorithm).
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let month_index = (month + 9) % 12;
    let day_of_year = (153 * month_index + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    let days = era * 146_097 + day_of_era - 719_468;
    Some(days * 86_400 + hour * 3_600 + minute * 60 + second)
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
    #[serde(default)]
    pub round: usize,
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
    let mut questions: Vec<GrillQuestion> = Vec::new();
    let mut current: Option<QuestionDraft> = None;

    for line in transcript.lines() {
        if skip_markdown_code_fence_line(line, &mut code_fence) {
            continue;
        }
        let line = line.trim();
        let (line, starts_agent_block) = strip_agent_block_marker(line);
        if let Some(header) = line.strip_prefix('❓') {
            if let Some(question) = current.take().and_then(QuestionDraft::finish) {
                questions.push(question);
            }
            let (number, title, prompt) = parse_question_header(header, questions.len() as u32 + 1);
            // Agents sometimes print a round, keep exploring, and print the
            // revised round again in the same turn. A number that does not
            // advance starts that newer round, so it supersedes the old one.
            if questions.last().is_some_and(|last| number <= last.number) {
                questions.clear();
            }
            current = Some(QuestionDraft::new(number, title, prompt));
            continue;
        }

        let Some(question) = current.as_mut() else {
            continue;
        };
        if starts_agent_block || is_separator_line(line) || line.starts_with('#') {
            question.section = DraftSection::Closed;
            continue;
        }
        question.push_line(line);
    }

    if let Some(question) = current.and_then(QuestionDraft::finish) {
        questions.push(question);
    }
    (!questions.is_empty()).then_some(GrillQuestionGroup {
        round: 0,
        questions,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DraftSection {
    /// Question body before the recommendation.
    Prompt,
    /// Wrapped continuation lines of the `➡️` recommendation.
    Recommendation,
    /// After the recommendation: options still attach, prose does not.
    Options,
    /// After a separator or a new agent block: nothing attaches.
    Closed,
}

struct QuestionDraft {
    question: GrillQuestion,
    section: DraftSection,
    paragraph_break: bool,
}

impl QuestionDraft {
    fn new(number: u32, title: Option<String>, prompt: String) -> Self {
        Self {
            question: GrillQuestion {
                number,
                title,
                prompt,
                recommendation: None,
                options: Vec::new(),
            },
            section: DraftSection::Prompt,
            paragraph_break: false,
        }
    }

    fn push_line(&mut self, line: &str) {
        if self.section == DraftSection::Closed {
            return;
        }
        if line.is_empty() {
            match self.section {
                DraftSection::Prompt => self.paragraph_break = true,
                DraftSection::Recommendation => self.section = DraftSection::Options,
                DraftSection::Options | DraftSection::Closed => {}
            }
            return;
        }
        if let Some(recommendation) = line.strip_prefix("➡️").or_else(|| line.strip_prefix('➡'))
        {
            let recommendation = clean_markup(recommendation);
            if !recommendation.is_empty() {
                self.question.recommendation = Some(recommendation);
                self.section = DraftSection::Recommendation;
            }
            return;
        }
        if let Some((key, label)) = parse_grill_option(line) {
            self.question.options.push(GrillOption { key, label });
            if self.section == DraftSection::Recommendation {
                self.section = DraftSection::Options;
            }
            return;
        }
        match self.section {
            DraftSection::Prompt => {
                let prompt = &mut self.question.prompt;
                if !prompt.is_empty() {
                    let separator = if self.paragraph_break {
                        "\n\n"
                    } else if starts_list_item(line) {
                        "\n"
                    } else {
                        " "
                    };
                    prompt.push_str(separator);
                }
                prompt.push_str(&clean_markup(line));
                self.paragraph_break = false;
            }
            DraftSection::Recommendation => {
                if let Some(recommendation) = self.question.recommendation.as_mut() {
                    recommendation.push(' ');
                    recommendation.push_str(&clean_markup(line));
                }
            }
            DraftSection::Options | DraftSection::Closed => {}
        }
    }

    fn finish(self) -> Option<GrillQuestion> {
        let mut question = self.question;
        if question.prompt.is_empty() {
            return None;
        }
        if question.title.is_none() {
            if let Some((title, prompt)) = split_leading_question(&question.prompt) {
                question.title = Some(title);
                question.prompt = prompt;
            }
        }
        Some(question)
    }
}

/// Terminal agents prefix each message or tool block with a marker (`•` in
/// Codex, `⏺` in Claude Code). Such a line starts a new block, which ends any
/// question that was being read.
fn strip_agent_block_marker(line: &str) -> (&str, bool) {
    ['•', '⏺', '●', '└']
        .iter()
        .find_map(|marker| line.strip_prefix(*marker))
        .map_or((line, false), |rest| (rest.trim_start(), true))
}

/// Markdown `---` rendered by terminal agents as `———`, `───`, and similar.
fn is_separator_line(line: &str) -> bool {
    line.chars().count() >= 3
        && line
            .chars()
            .all(|character| matches!(character, '-' | '—' | '–' | '─' | '━' | '_' | '*' | ' '))
}

fn starts_list_item(line: &str) -> bool {
    line.starts_with("- ")
        || line.starts_with("* ")
        || line.starts_with("• ")
        || line.split_once(". ").is_some_and(|(number, _)| {
            !number.is_empty() && number.chars().all(|c| c.is_ascii_digit())
        })
}

/// When an agent's renderer drops the bold title, use the leading question
/// sentence as the title so the dialog still reads as "title + detail".
fn split_leading_question(prompt: &str) -> Option<(String, String)> {
    let end = prompt.find('?')? + '?'.len_utf8();
    let (title, rest) = prompt.split_at(end);
    let rest = rest.trim();
    (title.chars().count() <= 120 && !title.contains('\n') && !rest.is_empty())
        .then(|| (title.trim().to_owned(), rest.to_owned()))
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

/// Whether a Pane capture still contains the previously captured transcript
/// as its prefix after removing terminal escape sequences.
pub fn grill_transcript_extends(previous_transcript: &str, transcript: &str) -> bool {
    let previous_transcript = strip_terminal_escape_sequences(previous_transcript);
    let transcript = strip_terminal_escape_sequences(transcript);
    transcript.starts_with(previous_transcript.as_str())
}

fn markdown_code_fence_state(transcript: &str) -> Option<(char, usize)> {
    let mut code_fence = None;
    for line in transcript.lines() {
        skip_markdown_code_fence_line(line, &mut code_fence);
    }
    code_fence
}

fn skip_markdown_code_fence_line(line: &str, code_fence: &mut Option<(char, usize)>) -> bool {
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

const HEADER_SEPARATORS: [char; 6] = ['-', '—', '–', ':', ')', '.'];

fn parse_question_header(header: &str, fallback_number: u32) -> (u32, Option<String>, String) {
    let header = header.trim_start_matches('\u{fe0f}').replace("**", "");
    let header = header.trim().trim_start_matches(HEADER_SEPARATORS).trim();
    let (number, rest) = parse_question_number(header).unwrap_or((fallback_number, header));
    let rest = rest.trim().trim_start_matches(HEADER_SEPARATORS).trim();
    let (title, prompt) = rest
        .split_once(':')
        .map(|(title, prompt)| (clean_markup(title), clean_markup(prompt)))
        .filter(|(title, prompt)| {
            !title.is_empty()
                && !prompt.is_empty()
                && !title.contains('?')
                && title.chars().count() <= 80
        })
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
        match characters.next() {
            Some('[') => {
                for character in characters.by_ref() {
                    if character.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
            // OSC (hyperlinks, titles) ends with BEL or ESC \.
            Some(']') => {
                while let Some(character) = characters.next() {
                    if character == '\u{7}' {
                        break;
                    }
                    if character == '\u{1b}' {
                        characters.next();
                        break;
                    }
                }
            }
            _ => {}
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
