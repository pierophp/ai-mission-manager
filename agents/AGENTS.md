# App-owned skills

Every skill in `skills/` is **self-contained**: the app injects only its `SKILL.md` into a run prompt, so everything the skill needs lives in that one file.

When a skill needs material from another skill, copy that material into its own `SKILL.md`, trimmed to what this skill uses. Never reference another skill by name, slash command, or path, and never point to a sibling file; neither reaches the agent at run time.

Every skill is **generic**: it runs against whatever repository the user targets, so it names roles ("the issue tracker", "the triage label", "the test suite") rather than a specific platform, tool, or command. The concrete choice comes from the target repository's own configuration or from the app's run prompt.

Every local change to a skill's `SKILL.md` is a **patch** on its upstream copy (named in the skill's `README.md`). In the same edit, record it as an entry in the skill's `PATCHES.md`: a heading, the change located by its `SKILL.md` section, and the reason, precise enough to re-apply to a fresh upstream copy without the old one at hand.

An upstream sync replaces `SKILL.md` with the new upstream version, then re-applies every entry in `PATCHES.md`; the sync is done when each patch is re-applied or removed because upstream now covers it.
