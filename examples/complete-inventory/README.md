# Complete inventory fixture

These files are harmless first-party fixtures, not collected extensions. No scripts, hooks or MCP
commands are executed by discovery/build. Initialize this directory as your own local Git source
before registering `local:<absolute-directory>`; the example registry is not an activated source.

The fixture covers a skill and its script companion, a native Claude agent, command, settings,
hooks contribution, MCP contribution, plugin metadata, assets and an unsupported vendor component.
Unsupported required behavior must fail rather than disappear. Native runtime activation still needs
separate explicit consent and a documented host contract. JSON configuration contribution metadata
can be provided in original Markdown frontmatter for exact native configuration merges.

Use [registry schema](../../schemas/registry.schema.json) for editor validation. Runtime validation
also checks portable paths, immutable content, bounds, dependency cycles and explicit trust.
The fixtures are licensed under this repository's GPL-3.0-only license. No external service or
credentials are required; the MCP command is a deliberately nonexistent first-party fixture name.
