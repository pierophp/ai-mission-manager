# Provider identity comes from the object's URL

Mission Manager derives an External Object's provider and kind from its URL (or, for a local Markdown object, its repository-relative path). A Context does not select one provider for all its work. It stores only the executable paths and non-secret target identifiers needed to invoke the provider CLIs; credentials remain in those CLIs' own authentication stores.

## Context

The initial design grouped a provider choice and its settings into each Context. That direction was reversed: an agent can publish different kinds of work from one Context, and the object URL already identifies the system that owns each Link. A provider selector would duplicate that identity and make a Context unable to represent mixed-provider work naturally.

## Decision

- Classify each external URL centrally to determine its provider and object kind. Keep local Markdown objects identifiable from their repository-relative paths.
- Let a Context configure CLI executable paths and target identifiers such as an Atlassian site, Azure DevOps organisation, or Bitbucket workspace. Do not store provider tokens or make the Context choose the provider of a Link.
- Dispatch reads and refreshes from the classified External Object. If its provider does not match the Context's configured target, retain the Link and surface a warning rather than discarding work the agent created.

## Consequences

- One Context can contain Links to objects from multiple providers while each External Object retains one stable provider identity.
- Credentials remain owned by the authenticated provider CLIs; Mission Manager stores no tokens.
- Context identifiers help aim and validate CLI calls but do not override the provider encoded by an object's URL.
- URL classification is a central domain boundary and needs tests for recognized and unrecognized URL shapes, canonical identity, and local paths.
