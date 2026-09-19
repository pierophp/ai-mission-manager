# Provider integration uses official CLIs that emit JSON

We shell out to already-authenticated provider CLIs instead of implementing OAuth. The criterion is not "CLI over API" but "does an official CLI with machine-readable output exist": `gh` for GitHub, and `acli` for Jira Cloud when that arrives. Bitbucket Cloud has no official CLI, so it will use the REST API with a stored token rather than an unofficial third-party binary; Azure DevOps is undecided.

## Considered Options

- **MCP servers as the integration surface.** Rejected: they return prose written for a model to read, not structured data for an application to consume, which is strictly worse than parsing a documented `--json` output, and remote MCP reintroduces the OAuth we were avoiding. MCP remains the right shape for the *reverse* direction — exposing AI Mission Manager to the agent running inside a Run — which is deferred.

## Consequences

v0 ships GitHub only. Of the four providers in the original design, `gh` is the one actually installed and authenticated; the others are setup work before they are integration work.
