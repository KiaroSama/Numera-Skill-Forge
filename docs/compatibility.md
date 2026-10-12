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

`scripts/verify-host-discovery.py` provides a bounded probe of already-installed Claude plugin
validation and Gemini extension/skill metadata discovery using separate harmless first-party profiles.
Positive and invalid/duplicate-name cases are distinct assertions. Claude's clean skill results are
omitted from diagnostic `contents`; a malformed first-party skill and manifest independently prove
actual traversal and rejection. This is metadata evidence, not a
model invocation or executable plugin activation. Profiles, environment and temporary files are
redirected; that alone is not an OS filesystem/network sandbox. It never invokes Claude details/eval,
Hermes self-healing startup or a collected skill. Missing hosts are not automatically installed.
On 2026-10-11, the installed Claude 2.1.295 positive/invalid-skill/invalid-manifest cases and Gemini
0.62.0 skill discovery/duplicate-name refusal passed in a Windows Job-owned run (18.1 seconds,
no leaked descendants, fixture removed). One Claude advisory and two Gemini security warnings for
ignored scratch system settings were retained. This evidence does not certify other host versions.

The direct Tauri driver supports Windows/Linux. A current macOS embedded WebDriver route exists
upstream, but requires an app automation plugin, changed permissions and a rebuilt test app; none is
installed here. Original macOS GUI acceptance instead uses an external Apple Accessibility client in
hosted-only CI (`scripts/native-macos-smoke.py` and `scripts/native-macos-ax.js`). It drives the unchanged
release through PID-scoped accessible controls, never an embedded server or injected app JavaScript.
Existing Accessibility trust and actual controls must be observed; missing trust/session/control
support fails without prompting or changing TCC. Implementation alone is not GUI/IPC certification.

The advertised 23 profiles span 18 host brands. A non-model command callback, metadata discovery,
activation and model-followed skill invocation are different claims. Installed Claude's `--init-only`
can run Setup/SessionStart without a conversation, but earlier credential/policy/MCP initialization
prevents treating it as inherently offline or credential-free. Gemini and Hermes regular startup
also have side effects beyond metadata inspection. Hosts not resolved on PATH are installation-
unverified, not proved absent, and are not installed automatically. Current acceptance explicitly
excludes private-account tests and model calls; all-host activation remains unverified without each
host's isolated supported runtime prerequisites.

Installation supports explicit project/user skill destinations, reviewed JSON configuration
contributions with three-way merging, and documented native-directory routes for Amp and Antigravity.
Owned files retain modes and shared references across update/disable/enable/uninstall/rollback.
Other native runtime packages still require their documented manager; Forge refuses a skill-only
install that would drop required behavior. Filesystem lifecycle tests do not certify host activation,
account permissions or every runtime plugin manager.
