# Local patches

Upstream source: `cursor/plugins/pstack` version `0.15.5`, commit `12d587dfb20741cafc376c42c696c5f6e2a64487`.

## Excluded upstream content

Omit `skills/make-bot-ui/`, `skills/automate-me/`, `skills/setup-pstack/`, `automations/benny/`, `docs/guide/`, and `.cursor-plugin/`. These paths are excluded by the app's vendor scope or depend on Cursor's plugin registry and setup flow.

## Harness support

Replace Cursor `Task` and `generalPurpose` assumptions with the host's native subagent mechanism. Map `poteto-agent` to a generic subagent that reads the full `poteto-mode` skill first. Adapt the upstream Harness section pattern from `backnotprop/pstack` to map Cursor names to Claude Code and Codex in the README. Restrict `/loop` to Claude Code and use Codex's native continuation controls. The app writes the tree to the execution machine and provides its absolute root path in the Run prompt.

## Removed Cursor facilities

Remove `/goal`, Cursor team kit, Bugbot, `environment: "cloud"`, and `cloud_base_branch` directions and references. Remove the Bugbot-specific triage reference. Keep review guidance harness-neutral.

## Prose cleanup and comment review

Replace `deslop` with `/unslop` plus `/no-comments` before review. Bundle the Comment Sicko prompt at `agents/comment-sicko.md` and have `/no-comments` pass that file to a generic subagent, so it does not need a host agent registry.

## Verification skills

Replace `control-ui` and `control-cli` with the repository's matching `verify-*` skill when one exists. When there is no matching skill, require the plan to name the manual interaction and observable result.

## Model roles

Replace `~/.cursor/rules/pstack-models.mdc` with instructions to read the app-generated role file from the absolute path supplied in the Run prompt. Do not encode an app-specific location in the vendored skills.

## Interrogate reviewer configuration

In `skills/interrogate/SKILL.md`, read the app-generated role file's `Review panel` entry and use its configured model, effort, and CLI invocation for Reviewer A. Keep the skill's other default-model reviewers so a pstack Review still uses multiple models. The app's role file names this configuration `Review panel` rather than `interrogate reviewers`.

## Harness-owned paths

Replace Cursor-only transcript and skill installation paths with paths supplied by the active harness or generic project-local locations. Remove the Cursor plugin manifest and omitted setup instructions.

## Poteto mode tree paths

Change plan skeleton paths from `pstack/skills/...` to `<pstack-root>/skills/...`, where `<pstack-root>` is the absolute tree path supplied by the Run prompt. This keeps the plan skeleton and `check-plan.mjs` invocation valid after the app writes the tree to another machine.

## Vendor integrity check

Add `scripts/check-pstack-vendor.mjs` to reject removed facility references and missing paths linked or named from `poteto-mode`. Add regression tests for forbidden references and missing paths, and run the tests and check from the root `npm test` command.
