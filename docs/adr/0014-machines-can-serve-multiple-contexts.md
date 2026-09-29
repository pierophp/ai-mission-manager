# Machines can serve multiple Contexts

An execution Machine is registered under one Context for management, but any Context may select it as its execution Machine. CLI configuration profiles remain attached to that Machine and can be reused by every Context assigned to it.

## Context

CLI configuration directories and their credentials live on a physical execution target. Requiring one Machine record per Context would prevent two Contexts that intentionally share a local or remote target from reusing the same provider profile.

## Decision

Keep a single Machine record for a target and allow multiple Contexts to select it. Context-level profile selections must reference profiles owned by the selected execution Machine and match the selected provider. Managing the Machine and its profiles remains in the Context where the Machine was registered.

## Consequences

- Contexts can share a provider identity without copying credentials or duplicating profile records.
- A Context still has at most one execution Machine, and an Item still belongs to one Context.
- Machine deletion affects every Context that selected it and must clear those Context selections.
- A Machine's registration Context remains the management boundary for its connection settings and profiles.
