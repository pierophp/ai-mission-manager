/// Build the pure Implement instructions for a queued GitHub sub-issue.
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
    format!(
        "{skill_body}\n\n## Ticket #{ticket_number}\n{ticket_url}\n\nRead the ticket using `gh issue view {ticket_number} --comments` before making changes.\n\nParent spec: {spec_url}\n\nReference #{ticket_number} in the commit message and close this sub-issue when the work is complete. Never close the parent spec or any other issue."
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
            object.id == start.spec_external_object_id
                && object.provider == super::ExternalProvider::GitHub
                && object.kind == super::ExternalObjectKind::Issue
        })
        .ok_or(DomainError::ImplementationSpecNotFound)?;
    if !state
        .links
        .iter()
        .any(|link| link.item_id == item_id && link.external_object_id == spec.id)
    {
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
                || entry.ticket_state.to_lowercase() != "open"
                || entry.ticket_number <= 0
                || entry.ticket_title.trim().is_empty()
                || !entry.ticket_url.starts_with("https://github.com/")
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
