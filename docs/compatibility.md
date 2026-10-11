# Compatibility

Capability records under [registry/capabilities](../registry/capabilities) and `numera-forge harness list`
are the machine-readable source. Their evidence is **static contract; real-host not tested**.

| Profile family | Current build treatment |
| --- | --- |
| Claude Code, Codex, Cursor, Antigravity, Gemini | Skills and matching native component payloads; preserve complete native plugins |
| Hermes | Portable skill/MCP subset; separate `hermes-native` preserves Python registration package |
| OpenCode v1/v2 | Separate versioned native source-directory preservation; no API translation |
| Copilot, VS Code, Kiro, OpenHands | Versioned portable subset, not all vendor runtime extensions |
| Pi, Amp, OpenClaw | Explicit resource/native-preservation profiles; executable code never imported by Forge |
| Windsurf/Devin Cascade, Cline, Kilo, Continue | Verified skill/config subsets; unverified runtime routes refuse |

Non-skill native content needs exact `native_target` identity or a verified host-specific path.
Generic hooks/agents directories cannot establish host compatibility. Required unsupported components
block generation. Optional exclusions must be explicit; no semantic conversion through renaming.

Codex stable portable loader hook/app limits, Cursor path variables, Antigravity unavailable schema,
OpenCode incompatible versions and managed-install restrictions are retained in capability records.
A syntactically valid JSON manifest is not proof that a host discovered or loaded it.

Installation supports explicit project/user skill destinations, reviewed JSON configuration
contributions with three-way merging, and documented native-directory routes for Amp and Antigravity.
Owned files retain modes and shared references across update/disable/enable/uninstall/rollback.
Other native runtime packages still require their documented manager; Forge refuses a skill-only
install that would drop required behavior. Filesystem lifecycle tests do not certify host activation,
account permissions or every runtime plugin manager.
