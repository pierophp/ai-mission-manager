/// Build the pure Implement instructions for one queued ticket.
pub fn compose_implementation_prompt(
    skill_document: &str,
    ticket_number: i64,
    ticket_url: &str,
    spec_url: &str,
) -> String {
    let skill_body = skill_document
        .strip_prefix("---")
        .and_then(|body| body.split_once("---").map(|(_, body)| body))
        .unwrap_or(skill_document)
        .trim();
    let read_command = ticket_read_command(ticket_number, ticket_url);
    let completion = ticket_completion_instruction(ticket_number, ticket_url);
    format!(
        "{skill_body}\n\n## Ticket #{ticket_number}\n{ticket_url}\n\nRead the ticket using `{read_command}` before making changes.\n\nParent spec: {spec_url}\n\n{completion}"
    )
}

fn ticket_read_command(ticket_number: i64, ticket_url: &str) -> String {
    if let Some(reference) = ticket_url.strip_prefix("local:") {
        if let Some((_, path)) = reference.split_once('#') {
            return format!("cat {}", shell_quote(path));
        }
    }
    if let Some(path) = ticket_url.strip_prefix("file://") {
        let path = percent_decode(path);
        return format!("cat {}", shell_quote(&path));
    }
    if ticket_url.contains("github.com/") {
        return format!("gh issue view {ticket_number} --comments");
    }
    if let Some(key) = ticket_url
        .split("/browse/")
        .nth(1)
        .and_then(|tail| tail.split(['?', '#', '/']).next())
    {
        let site = ticket_url.split('/').nth(2).unwrap_or_default();
        return format!("twg jira workitem get {key} --site https://{site} --output json");
    }
    format!("read {ticket_url} (ticket #{ticket_number})")
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            if let (Some(high), Some(low)) = (
                (bytes[index + 1] as char).to_digit(16),
                (bytes[index + 2] as char).to_digit(16),
            ) {
                decoded.push(((high << 4) | low) as u8);
                index += 3;
                continue;
            }
        }
        decoded.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&decoded).into_owned()
}

fn ticket_completion_instruction(ticket_number: i64, ticket_url: &str) -> String {
    if ticket_url.starts_with("local:") || ticket_url.starts_with("file://") {
        return format!(
            "Reference #{ticket_number} in the commit message and set this ticket's `Status:` line to `Closed` when the work is complete. Never change the parent Spec's status."
        );
    }
    if ticket_url.contains("github.com/") {
        return format!(
            "Reference #{ticket_number} in the commit message and close this sub-issue when the work is complete. Never close the parent spec or any other issue."
        );
    }
    format!(
        "Reference #{ticket_number} in the commit message and mark this ticket complete in its provider when the work is complete. Never close or change the parent spec."
    )
}

/// Build the prompt shared by each Implementation Queue entry.
pub fn compose_implementation_queue_prompt(
    ticket_number: i64,
    ticket_url: &str,
    spec_url: &str,
) -> String {
    compose_implementation_prompt(
        crate::domain::implementation_skill_snapshot(),
        ticket_number,
        ticket_url,
        spec_url,
    )
}

pub fn implementation_ticket_is_open(state: &str) -> bool {
    !matches!(
        state.trim().to_ascii_lowercase().as_str(),
        "closed" | "done" | "resolved" | "completed" | "removed" | "cancelled" | "canceled"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_skill_frontmatter_and_scopes_issue_closure() {
        let prompt = compose_implementation_prompt(
            "---\nname: implement\n---\nDo the work.",
            42,
            "https://github.com/o/r/issues/42",
            "https://github.com/o/r/issues/87",
        );
        assert!(prompt.starts_with("Do the work."));
        assert!(!prompt.contains("name: implement"));
        assert!(prompt.contains("gh issue view 42 --comments"));
        assert!(prompt.contains("Never close the parent spec or any other issue."));
    }

    #[test]
    fn implementation_prompt_contains_the_local_implement_instructions_without_metadata() {
        let prompt = compose_implementation_queue_prompt(
            42,
            "https://github.com/o/r/issues/42",
            "https://github.com/o/r/issues/87",
        );

        assert!(
            prompt.starts_with("Implement the work described by the user in the spec or tickets.")
        );
        assert!(!prompt.contains("name: implement"));
        assert!(prompt.contains("Run typechecking regularly, single test files regularly"));
    }

    #[test]
    fn supported_provider_prompts_use_concrete_read_commands_without_mentioning_gh() {
        for (number, url, command) in [
            (
                42,
                "https://acme.atlassian.net/browse/APP-42",
                "twg jira workitem get APP-42 --site https://acme.atlassian.net --output json",
            ),
            (
                44,
                "local:7#.scratch/demo/issues/44-ticket.md",
                "cat '.scratch/demo/issues/44-ticket.md'",
            ),
            (
                45,
                "file:///repo/.scratch/demo/issues/45-my%20ticket.md",
                "cat '/repo/.scratch/demo/issues/45-my ticket.md'",
            ),
        ] {
            let prompt = compose_implementation_prompt("Implement it.", number, url, "spec");
            assert!(
                prompt.contains(&format!("Read the ticket using `{command}`")),
                "{url}"
            );
            assert!(!prompt.contains("gh issue view"), "{url}");
            assert!(
                prompt.contains("mark this ticket complete in its provider")
                    || prompt.contains("Status:` line to `Closed"),
                "{url}"
            );
        }
    }

    #[test]
    fn github_prompt_keeps_its_existing_read_and_close_instructions() {
        let prompt = compose_implementation_prompt(
            "Implement it.",
            42,
            "https://github.com/o/r/issues/42",
            "https://github.com/o/r/issues/87",
        );
        assert!(prompt.contains("gh issue view 42 --comments"));
        assert!(prompt.contains("close this sub-issue when the work is complete"));
        assert!(prompt.contains("Never close the parent spec or any other issue."));
    }

    #[test]
    fn provider_terminal_states_count_as_closed_while_active_states_remain_open() {
        for state in ["Open", "Active", "To Do", "In Progress"] {
            assert!(implementation_ticket_is_open(state), "{state}");
        }
        for state in ["Closed", "Done", "Resolved", "Completed", "Removed"] {
            assert!(!implementation_ticket_is_open(state), "{state}");
        }
    }
}
use super::{
    validate_grill_configuration, DomainError, DomainState, GrillConfiguration,
    ImplementationQueue, ImplementationQueueStart,
};

/// Validate and create the queue snapshot used by the first gated Run.
pub fn create_implementation_queue(
    state: &DomainState,
    queue_id: i64,
    item_id: i64,
    workspace_id: i64,
    repository_id: i64,
    configuration: &GrillConfiguration,
    allow_dirty: bool,
    allow_shared_checkouts: bool,
    start: ImplementationQueueStart,
) -> Result<ImplementationQueue, DomainError> {
    if state
        .implementation_queues
        .iter()
        .any(|queue| queue.item_id == item_id && queue.active)
    {
        return Err(DomainError::ImplementationQueueAlreadyActive { item_id });
    }
    validate_grill_configuration(configuration)?;
    let spec = state
        .external_objects
        .iter()
        .find(|object| {
            object.id == start.spec_external_object_id && is_implementation_spec_object(object)
        })
        .ok_or(DomainError::ImplementationSpecNotFound)?;
    if !state.links.iter().any(|link| {
        link.item_id == item_id
            && link.external_object_id == spec.id
            && link.purpose == super::LinkPurpose::ToSpec
    }) {
        return Err(DomainError::ImplementationSpecNotLinked);
    }
    if start.spec_url != spec.canonical_url {
        return Err(DomainError::ImplementationSpecUrlMismatch);
    }
    let mut entries = start.entries;
    entries.sort_by_key(|entry| entry.position);
    if entries.is_empty()
        || entries.iter().enumerate().any(|(index, entry)| {
            entry.position != index as i64
                || !implementation_ticket_is_open(&entry.ticket_state)
                || entry.ticket_number <= 0
                || entry.ticket_title.trim().is_empty()
                || !is_implementation_ticket_url(&entry.ticket_url)
        })
        || entries.iter().enumerate().any(|(index, entry)| {
            entries[..index].iter().any(|previous| {
                previous.ticket_number == entry.ticket_number
                    || previous.ticket_url == entry.ticket_url
            })
        })
    {
        return Err(DomainError::InvalidImplementationQueueEntries);
    }
    for entry in &mut entries {
        entry.run_id = None;
        entry.done = false;
        entry.skipped = false;
    }
    Ok(ImplementationQueue {
        id: queue_id,
        item_id,
        spec_external_object_id: spec.id,
        spec_url: start.spec_url,
        workspace_id,
        repository_id,
        configuration: configuration.clone(),
        allow_dirty,
        allow_shared_checkouts,
        entries,
        active: true,
        paused_reason: None,
    })
}

fn is_implementation_ticket_url(url: &str) -> bool {
    url.starts_with("local:")
        || url.starts_with("file://")
        || (url.starts_with("https://github.com/") && url.contains("/issues/"))
        || (url.starts_with("https://") && url.contains(".atlassian.net/browse/"))
}

pub(super) fn is_implementation_spec_object(object: &super::ExternalObject) -> bool {
    (object.provider == super::ExternalProvider::GitHub
        && object.kind == super::ExternalObjectKind::Issue)
        || (object.provider == super::ExternalProvider::Atlassian
            && matches!(
                object.kind,
                super::ExternalObjectKind::Issue | super::ExternalObjectKind::Document
            ))
        || (object.provider == super::ExternalProvider::Generic
            && object.external_key.starts_with("local:"))
}

pub(super) fn is_implementation_ticket_object(object: &super::ExternalObject) -> bool {
    (object.provider == super::ExternalProvider::GitHub
        && object.kind == super::ExternalObjectKind::Issue)
        || (object.provider == super::ExternalProvider::Atlassian
            && object.kind == super::ExternalObjectKind::Issue)
        || (object.provider == super::ExternalProvider::Generic
            && object.external_key.starts_with("local:"))
}
