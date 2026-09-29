# Provider integration uses official CLIs that emit JSON

We shell out to provider CLIs that already own authentication instead of implementing OAuth. The criterion is not "CLI over API" but "does an official CLI with machine-readable output exist?" The current integrations are `gh` for GitHub, `twg` for Jira, Confluence and Bitbucket, and `az` with the `azure-devops` extension for Azure DevOps. These tools are invoked with JSON output where supported. Mission Manager stores executable paths and target identifiers, but no provider tokens.

## Decision history

The original decision was to use `gh` for GitHub, consider `acli` for Jira Cloud, use REST plus a stored token for Bitbucket because it was believed to have no official CLI, and leave Azure DevOps undecided. That prediction was wrong: `twg` is an official Atlassian CLI covering Jira, Confluence and Bitbucket, and the official Azure DevOps CLI has an `azure-devops` extension. Both meet the original machine-readable-output criterion, so the criterion stands and these provider choices replace the earlier predictions.

## Considered Options

- **MCP servers as the integration surface.** Rejected: they return prose written for a model to read, not structured data for an application to consume, which is strictly worse than parsing a documented `--json` output, and remote MCP reintroduces the OAuth we were avoiding. MCP remains the right shape for the *reverse* direction — exposing AI Mission Manager to the agent running inside a Run — which is deferred.

## Consequences

- Provider integrations use the official CLI's existing authentication rather than implementing or storing provider credentials.
- The app can use the same adapter shape for each CLI, while each Context supplies the executable path and target identifier needed by that installation.
- The original REST-plus-token prediction for Bitbucket and undecided status for Azure DevOps are superseded by the current integrations above.
- MCP remains the right shape for the reverse direction — exposing AI Mission Manager to the agent running inside a Run — which is deferred.
