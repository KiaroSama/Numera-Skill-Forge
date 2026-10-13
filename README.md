<p align="center">
  <img src="assets/logo-round.png" alt="Numera Skill Forge official logo" width="240">
</p>

<h1 align="center">Numera Skill Forge</h1>

<div align="center">

[![CI](https://github.com/KiaroSama/Numera-Skill-Forge/actions/workflows/ci.yml/badge.svg)](https://github.com/KiaroSama/Numera-Skill-Forge/actions/workflows/ci.yml)
[![License GPL v3](https://img.shields.io/badge/License-GPL_v3-blue)](LICENSE)
[![Rust 1.99](https://img.shields.io/badge/Rust-1.99-000000?logo=rust)](rust-toolchain.toml)
[![TypeScript](https://img.shields.io/badge/TypeScript-strict-3178C6?logo=typescript)](apps/desktop/tsconfig.json)
[![Tauri 2](https://img.shields.io/badge/Tauri-2-24C8D8?logo=tauri)](apps/desktop/src-tauri/Cargo.toml)
[![Vite](https://img.shields.io/badge/Vite-8-646CFF?logo=vite)](apps/desktop/package.json)
[![CLI](https://img.shields.io/badge/Interface-headless_CLI-0A7BBB)](crates/forge-cli)
[![Desktop](https://img.shields.io/badge/Interface-desktop-0A7BBB)](apps/desktop)
[![UTF-8](https://img.shields.io/badge/Text-UTF--8-0A7BBB)](docs/architecture.md)
[![Docs](https://img.shields.io/badge/Documentation-guides-0A7BBB)](docs)
[![SHA-256](https://img.shields.io/badge/Integrity-SHA--256-0A7BBB)](crates/forge-core/src/storage.rs)
[![JSON](https://img.shields.io/badge/Automation-JSON-0A7BBB)](docs/installation.md)
[![Support donations](https://img.shields.io/badge/Support-donations-d04a9a)](#donate)

</div>

Source-aware distribution and synchronization for coding-harness extensions: **Skills, Agents,
Commands, Settings, Hooks, MCPs, Mods and Plugins**, plus rules, workflows, tools, language tooling,
styles, UI extensions, themes, services, providers, channels, onboarding contracts and assets.

**Development preview, not a complete production release.** The current executable foundation
registers and inspects licensed Git sources, explicitly approves content, deterministically builds
dependency-closed packages, and performs receipt-based project/user installation, managed JSON
configuration merging, disable/enable/uninstall and mode-preserving journal recovery. Broader native component adapters and platform runtime
certification remain in progress. A listed harness is not a claim that every component works there.

## Quick start

Source builds need Rust 1.99, Node 24, Git and [Tauri platform prerequisites](https://v2.tauri.app/start/prerequisites/).
The headless CLI does not require a webview. No LLM subscription or API key is required.

```powershell
cargo run -p forge-cli -- --repository . init
cargo run -p forge-cli -- --repository . source list
cargo run -p forge-cli -- --repository . harness list
cargo run -p forge-cli -- --repository . doctor --json
```

macOS/Linux use the same Cargo commands in their shell. Windows native usage does not require WSL.
WSL is a separate Linux environment: select its own workspace and destinations.

## First skill collection

The native Claude marketplace currently packages **485 reviewed skills** from
[davila7/claude-code-templates](https://github.com/davila7/claude-code-templates), each as a separate
plugin, with original prompts, scripts, companions and notices unchanged. This is a bounded licensed
subset, not the entire upstream collection. See [licenses and exclusions](docs/source-licenses.md).

```bash
claude plugin marketplace add KiaroSama/Numera-Skill-Forge
```

```bash
claude plugin install skill-creative-design-frontend-design@numera-skill-forge
```

Installing/enabling a skill is separate consent; some skills contain scripts or use external services.
Forge never executes imported code to inspect, package or synchronize it.

## Separate additional collections

Two independent plugins bring the marketplace total to **487**; the existing 485-plugin collection
above remains unchanged. Each retains the original skill, every in-root reference and its MIT notice.

| Collection | Skill | Independent plugin |
| --- | --- | --- |
| [JCodesMore website cloner](https://github.com/JCodesMore/ai-website-cloner-template) | `clone-website` | `skill-jcodesmore-clone-website` |
| [morluto REA](https://github.com/morluto/rea) | `reverse-engineer-anything` | `skill-morluto-reverse-engineer-anything` |

```bash
claude plugin install skill-jcodesmore-clone-website@numera-skill-forge
```

```bash
claude plugin install skill-morluto-reverse-engineer-anything@numera-skill-forge
```

These are instruction packages, not copies of the upstream applications. REA's skill does not
register its MCP server, install analysis engines or include a commercial-tool license. Its external
runtime/tool prerequisites remain separate. The two repository-specific REA maintainer skills are
not included. Both selected collections use the same check/apply, accepted-revision and review-only
update rules below; an upstream change requiring review retains the accepted version.

## Daily and local updates

[Collection synchronization](docs/upstream-sync.md) runs daily at **03:37 UTC** and can be started
manually in GitHub Actions. Safe reviewed prompt updates are rebuilt and pushed; script, license,
file-structure and executable-mode changes require review. The local [run.ps1](run.ps1) launcher
checks by default; `-Apply` updates accepted collection files, never installs extensions into a host.

```powershell
./run.ps1 -Check -Build
```

```powershell
./run.ps1 -Apply
```

Register additional repositories only after approval; public availability alone is not a redistribution
license. See [Adding content](docs/source-format.md).

## Easy additions

For an existing supported component/target profile, adding another skill/plugin is a registration
change, not an engine rewrite. Register its source, inspect its full companions and license, approve
the reviewed digest, then add a package record. New host APIs still require reviewed adapters/tests.

```powershell
numera-forge source add https://github.com/OWNER/REPOSITORY --id selected-source --root skills/selected
numera-forge discover selected-source --json
numera-forge approve selected-source --digest REVIEWED_DIGEST
numera-forge package add package.json
numera-forge build selected-package --target claude-code --locked
```

These placeholders are examples, not enabled sources. Executable content requires separate explicit
trust; `--yes` is not a trust bypass.

## Maintainer and consumer boundaries

Consumers do not need write access to this repository. Project installs require an explicit destination
and support dry-run. Configuration/native-runtime installation routes are distinct from skill copies.
Publication defaults to preview; local updates are not GitHub updates. No auto scheduler or daemon.
Desktop reopening checks enabled sources sequentially, with failed sources retaining accepted state.
Sources exposes explicit per-source update policy and enabled-state controls, with reviewed confirmation.
Settings can separately opt into policy-approved safe updates and rebuilds on open; it never installs or
publishes. Generic builds emit target catalogs and a separate Claude marketplace without changing the
collection-owned repository marketplace; native manifest identities and user-edited outputs are preserved. Installed updates coordinate all enabled owners per destination and preserve user changes.
Persisted per-step progress supports inspection/cancellation and failed-only retry.

## Documentation

- [Architecture and current limitations](docs/architecture.md)
- [Installation and CLI](docs/installation.md)
- [Source and package registration](docs/source-format.md)
- [Compatibility](docs/compatibility.md)
- [Verification](docs/testing.md)

## Reference repositories

See **[all reference repositories](docs/reference-repositories.md)** for the complete public link list,
separated into consulted contracts and unqualified example candidates. Listing is not source approval.

## Logs and privacy

Each application run writes a new UTF-8 UTC file under the selected workspace's local state log
directory, named `numera-forge_YYYY-MM-DD_HH-mm-ss_UTC.log` with collision-safe suffixes. Entries
contain UTC timestamps, INFO/WARNING/ERROR/DEBUG and categories. DEBUG is opt-in with `NUMERA_DEBUG=1`.
Commands/results go to stdout; diagnostics go to stderr/files. Credentials and raw upstream process
output are not logged. Telemetry is disabled. Review/redact logs before sharing; automatic support
bundles are local-only (`support`, logs opt-in); `log-retention` previews by default and requires
`--apply` to remove older logs while retaining the requested newest files.

## Donate

If this project helps you, donations are appreciated.

| Currency | Network | Address |
| --- | --- | --- |
| Bitcoin (BTC) | Bitcoin | `bc1qmth5m03pu5hujw5xw5jmywam3jj3sqwqupesdt` |
| USDT, BNB, USDC, etc. | BEP20 | `0x0Bd0BA443a8B9cf15922bf7f0Bb0a4b495fD06Ef` |
| USDT, TRX, USDC, etc. | TRC20 | `TWBA3xFTqgZAeAYMxqo85xWnzvty3DcAhw` |
| Ethereum (ETH) | ERC20 | `0x0Bd0BA443a8B9cf15922bf7f0Bb0a4b495fD06Ef` |
| TON | TON | `UQCN8Umo_OfOWqImZetQsrNStPcmLkMAKajFyiCOhso23NDb` |
| Litecoin (LTC) | LTC | `ltc1qntqnnrunadurnw4cshv3qgspywrueyyeyngwuy` |
| Solana (SOL) | Solana | `7B2wkczUjmkDhETwQuknBL8sUsbuV7nErxc317TmQuwR` |
| Polygon (POL) | Polygon | `0x0Bd0BA443a8B9cf15922bf7f0Bb0a4b495fD06Ef` |

## License

The existing [GNU GPL Version 3 license](LICENSE) is preserved. First-party code uses GPL-3.0-only;
third-party payloads keep their own licenses/notices and require reviewed redistribution rights.
