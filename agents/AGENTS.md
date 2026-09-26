# App-owned skills

Every skill in `skills/` is **self-contained**: the app injects only its `SKILL.md` into a run prompt, so everything the skill needs lives in that one file.

When a skill needs material from another skill, copy that material into its own `SKILL.md`, trimmed to what this skill uses. Never reference another skill by name, slash command, or path, and never point to a sibling file; neither reaches the agent at run time.

Every skill is **generic**: it runs against whatever repository the user targets, so it names roles ("the issue tracker", "the triage label", "the test suite") rather than a specific platform, tool, or command. The concrete choice comes from the target repository's own configuration or from the app's run prompt.

Record each local divergence from upstream in the skill's `README.md`, so a manual upstream sync keeps it.
