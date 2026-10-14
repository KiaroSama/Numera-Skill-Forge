# Host Setup: Native Plugins and Original Skills

All accepted collection packages are selectable independently. Forge preserves original SKILL.md,
companions, notices and executable modes; it never installs their dependencies or runs their scripts.
A valid descriptor, copied skill, host discovery, enablement and model invocation are separate results.

## 1. Get the distribution

Clone this repository, or update your existing clean checkout. Install the host separately using its
official instructions. Use an unsigned Forge release only within your operating system's normal
protections, or [build the CLI](installation.md). Commands below assume `numera-forge` is on PATH
and run from this repository; otherwise pass its absolute path and `--repository <forge-checkout>`.

```bash
git clone https://github.com/KiaroSama/Numera-Skill-Forge.git
```

List accepted package IDs:

```bash
numera-forge catalog list --json
```

Examples used below:
- `skill-jcodesmore-clone-website`: original `clone-website`.
- `skill-morluto-reverse-engineer-anything`: original `reverse-engineer-anything`.
- `skill-newliver666-apk-reverse`: original `apk-reverse`; the same native catalog and standalone
  commands apply when this ID is substituted. Its complete scripts/references remain unchanged.
  Python 3.9+ and Android/ADB/Frida tools are conditional upstream prerequisites, not Forge installs.
  Read the [original requirements and authorized-use scope](https://github.com/newliver666/apk-reverse#requirements)
  before execution; packaging/discovery is not an APK analysis or device test.

K-Dense packages use `skill-kdense-<original-name>` for all177 original skills, for example
`skill-kdense-statistical-analysis`. Substitute that package ID in the native or standalone
commands below. This is an independent collection; updates retain existing content-review gates.
Original scientific dependencies/services remain optional external prerequisites. Same-name skills
from other sources are not renamed or overwritten; use separate standalone destinations or the
host's supported plugin namespaces. A metadata limit on a particular host remains an explicit refusal.

The REA analysis engines/MCP server and third-party commercial tools are not installed by this guide.
Some other packages likewise require external tools or services; inspect their original instructions
before enabling them. Public availability is not blanket execution authorization.

## 2. Native plugin catalogs

Every entry references an independent package under `packages/claude-code/<package-id>`.
The directory name is a historical storage location, not a Claude-only payload limitation.
There is no giant aggregate plugin and no renamed original skill.

| Host | Repository catalog | Package descriptor |
| --- | --- | --- |
| Claude Code | `.claude-plugin/marketplace.json` | `.claude-plugin/plugin.json` |
| Codex | Supported `.claude-plugin/marketplace.json` fallback | Portable `plugin.json` preferred; `.codex-plugin/plugin.json` compatibility fallback |
| Cursor | `.cursor-plugin/marketplace.json` | `.cursor-plugin/plugin.json` |
| Portable Agent Plugins consumers | Host's documented local import | `plugin.json`, conventional `skills/` |

### Claude Code

Add the repository once:

```bash
claude plugin marketplace add KiaroSama/Numera-Skill-Forge
```

Install one independent package:

```bash
claude plugin install skill-jcodesmore-clone-website@numera-skill-forge
```

Use the host's plugin list/manager to verify enabled state and start a new session. Plugin skill names
are qualified by package identity. Update/remove through the same native manager; Forge receipts do
not own files installed by that manager. [Official plugin reference](https://code.claude.com/docs/en/plugins-reference).

### Codex

Open Codex's plugin manager and add this GitHub repository as a marketplace source. Select the
individual package in `numera-skill-forge`, install, and explicitly enable as required by your client.
Codex recognizes this repository's Claude/Cursor native catalog fallbacks. Its canonical
`.agents/plugins/marketplace.json` location is deliberately not shipped: repository protection reserves
that directory for private agent state. There is no invented `.codex-plugin/marketplace.json` or
permission bypass; the supported fallback preserves all independent package entries.

Verify the package ID and its qualified skill (for example,
`skill-jcodesmore-clone-website:clone-website`) in the installed inventory. Restart the session if the
client requires it. Use that manager for updates/removal. Native inventory/GUI behavior depends on
the installed client; Forge metadata checks are not a claim that your client was run.
[Official build/catalog documentation](https://developers.openai.com/codex/plugins/build).

### Cursor

Use Cursor's Plugins interface with this repository's native marketplace when the installed client
supports repository sources. Choose an individual package, review its permissions and enable it.
Public marketplace approval is separate; this repository does not claim official marketplace listing.
If your client cannot add this repository, use standalone installation below, not a fabricated CLI.

Check the skill in Cursor's skill inventory and start a new agent session. Catalog entries preserve
package identity, but simultaneous same-name skill invocation is not certified by a JSON validator.
Use the manager for manager-owned updates/removal.
[Plugin reference](https://cursor.com/docs/reference/plugins), [skills](https://cursor.com/docs/context/skills).

### Hermes portable packages

For the default profile, install a selected local package disabled first:

```bash
hermes plugins install ./packages/claude-code/skill-jcodesmore-clone-website --no-enable
```

Inspect `hermes plugins list`, then explicitly enable the returned plugin identity through Hermes.
Portable plugin skills are namespaced; original skill files remain unchanged. No Python registration
stub or skill-bundle alias is generated. Remove/update through Hermes's plugin manager. Custom
profiles use their actual selected HERMES_HOME, not another profile's files.
[Plugins](https://hermes-agent.nousresearch.com/docs/developer-guide/plugins/).

## 3. Standalone skills through Forge

This route uses Forge's ownership receipts, preview, safe updates, disable/enable, uninstall and
rollback. It does **not** register a native runtime plugin, MCP server, hook or host configuration.
Choose one target from the next table and an **existing absolute** project or home directory.
Never select Forge's checkout as a destination unless it is genuinely the project you want to extend.

Example Antigravity project preview:

```bash
numera-forge install skill-jcodesmore-clone-website --target antigravity --scope project --destination /absolute/path/to/project --dry-run --json
```

After reviewing that exact preview, apply by omitting `--dry-run`:

```bash
numera-forge install skill-jcodesmore-clone-website --target antigravity --scope project --destination /absolute/path/to/project --json
```

Example OpenCode v2 user preview (destination is the actual home directory):

```bash
numera-forge install skill-morluto-reverse-engineer-anything --target opencode-v2 --scope user --destination /absolute/path/to/home --dry-run --json
```

On Windows use a quoted absolute Windows path, e.g. `"D:/Projects/My Project"`; no Unix home expansion
is assumed by Forge. Use `--scope user` only when the selected root is the intended home. Custom host
profile roots/configured workspaces require the documented surface below; Forge does not guess them.

### Destination matrix

Paths are relative to the selected project or selected home, not the shell's CWD. Each directory
contains `<original-skill-name>/SKILL.md` and the unchanged companion tree.

| Target | Project prefix | User/home prefix | Setup and discovery |
| --- | --- | --- | --- |
| `claude-code` | `.claude/skills` | `.claude/skills` | New Claude session; inspect available skills. |
| `codex` | `.agents/skills` | `.agents/skills` | New Codex session; native plugin route preferred for duplicate package identities. |
| `cursor` | `.cursor/skills` | `.cursor/skills` | Local editor/agent skill inventory; nested project scope applies to descendants. |
| `antigravity` | `.agents/skills` | `.gemini/config/skills` | **Desktop** surface; reload/start a new agent session and inspect skills. |
| `opencode`, `opencode-v1` | `.opencode/skills` | `.config/opencode/skills` | v1: `skill({name})`; names unique and directory-matched. |
| `opencode-v2` | `.opencode/skills` | `.config/opencode/skills` | v2: enclosing directory is ID; `skill({id})`/`@skill-id`; frontmatter name is display. |
| `hermes` | `.hermes/skills` | `.hermes/skills` | Human project trust required; default profile home only. |
| `copilot` | `.github/skills` | `.copilot/skills` | Copilot CLI session/inventory; not an editor extension package. |
| `vscode` | `.github/skills` | `.copilot/skills` | Local VS Code Copilot agent customization/skill inventory. |
| `gemini` | `.gemini/skills` | `.gemini/skills` | Gemini skill inventory; project/user/extension precedence matters. |
| `amp` | `.agents/skills` | `.config/agents/skills` | Amp skill inventory/new session; no native JS/TS bundle registration. |
| `pi` | `.pi/skills` | `.pi/agent/skills` | `/reload`, then `/skill:<original-name>`; this is text delivery, not an extension. |
| `kiro` | `.kiro/skills` | `.kiro/skills` | Reload/inspect skills; custom agents may require explicit skill resources. |
| `windsurf` | `.devin/skills` | `.config/devin/skills` | Cascade surface; not Devin Local CLI certification. |
| `cline` | `.cline/skills` | `.cline/skills` | Enable/discover skills in the installed Cline surface; global names can override project. |
| `kilo` | `.kilo/skills` | `.kilo/skills` | Documented desktop skill surface; do not infer CLI native plugin support. |
| `continue` | `.continue/skills` | `.continue/skills` | Continue CLI default home; restart session and inspect skills. |
| `openhands` | `.agents/skills` | `.agents/skills` | Selected SDK workspace; actual SDK skill discovery, not native plugin import. |
| `openclaw` | `skills` | `.openclaw/skills` | Project destination MUST be configured agent workspace; user default home only. |

Hermes project trust is a separate human decision, not performed by Forge:

```bash
hermes skills trust /absolute/path/to/project
```

Antigravity CLI uses a different global path (`~/.gemini/antigravity-cli/skills`); the existing
`antigravity` profile is desktop and must not be used to claim that CLI global setup. For custom Hermes
HERMES_HOME, Continue home or OpenClaw state-dir, use the host's documented profile installation;
the user-scope defaults above do not silently become custom-state paths.

`hermes-native`, `amp-native` and `openclaw-native` are executable-native profiles, not extra skill
brands. Use the corresponding ordinary skill target for these collections. Forge refuses skill-copy
conversion of a native package when it would discard required runtime behavior.

### Verification, updates and removal

Inspect the install result/receipt and actual host inventory. File presence alone proves neither
host loading nor tool/script portability. If a host rejects original metadata, Forge keeps the
original unchanged and reports the incompatibility; do not edit it merely to obtain a green check.

```bash
numera-forge installed --json
```

Synchronize approved upstream changes with the existing collection rules:

```bash
numera-forge collection sync --apply --json
```

Update enabled Forge-owned installs coherently:

```bash
numera-forge update --installed --json
```

Use the returned receipt ID for disable/enable/uninstall; these touch only unchanged owned files.
Native-manager installs are not Forge receipts and must use that manager instead.

```bash
numera-forge disable RECEIPT_ID
```

```bash
numera-forge enable RECEIPT_ID
```

```bash
numera-forge uninstall RECEIPT_ID
```

Rollback takes the returned operation ID and refuses changed current files:

```bash
numera-forge rollback OPERATION_ID
```

Safe prompt-body updates retain existing approval rules; script/license/topology/mode changes stay
review-only. A failed upstream source retains its last-known-good package. Never overwrite edited
host files or bypass an ownership conflict with a manual bulk copy.

## 4. Duplicate names and evidence limits

There are 487 accepted packages and 472 distinct original names. Thirteen groups share names.
Native per-package namespaces retain independent identity where the host implements it. Flat
standalone roots cannot make different same-name packages simultaneously unambiguous: Forge refuses
the second owner, even when their main SKILL.md bytes match. Select another project/home destination
or uninstall the prior owner first. No original skill name is rewritten or silently dropped.

Current verification distinguishes deterministic metadata/path checks, first-party scoped lifecycle
fixtures, existing original host SDK/API evidence and unavailable real editor/CLI runtime evidence.
No all-host model invocation, personal account setup, cloud propagation or public marketplace listing
is certified. Original companion tools can have additional OS/runtime/license requirements.
See [compatibility](compatibility.md) and [testing](testing.md).

## Official skill contracts (checked 2026-10-13)

- [Antigravity](https://antigravity.google/docs/skills), [OpenCode v1](https://opencode.ai/docs/skills/), [OpenCode v2 source guide](https://github.com/anomalyco/opencode/blob/1015cb67d08858496518b02f437b8e4a2fc89d61/services/www/src/docs/content/skills.mdx), [Hermes](https://hermes-agent.nousresearch.com/docs/user-guide/features/skills/).
- [Copilot CLI](https://docs.github.com/en/copilot/how-tos/copilot-cli/customize-copilot/create-skills), [VS Code](https://code.visualstudio.com/docs/copilot/customization/agent-skills), [Gemini](https://geminicli.com/docs/cli/skills/), [Amp](https://ampcode.com/docs/markdown/customize/skills).
- [Pi v1.2.0](https://github.com/earendil-works/pi/blob/v1.2.0/packages/coding-agent/docs/skills.md), [Kiro](https://kiro.dev/docs/skills/), [Windsurf](https://docs.windsurf.com/windsurf/cascade/skills), [Cline](https://docs.cline.bot/customization/skills).
- [Kilo](https://kilo.ai/docs/customize/skills), [Continue loader](https://github.com/continuedev/continue/blob/5522c6f44ca0ac3528b37244818fbfa39b5af470/extensions/cli/src/util/loadMarkdownSkills.ts), [OpenHands](https://docs.openhands.dev/overview/skills.md), [OpenClaw](https://docs.openclaw.ai/tools/skills).
