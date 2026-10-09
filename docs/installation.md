# Installation and CLI

## Source development

Use Rust 1.99.0, Node 24 and Git. Desktop source builds additionally need the current
[Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) (Windows MSVC/WebView2,
macOS Xcode tools, Linux GTK/WebKit development libraries). The CLI needs no display server.

```powershell
cargo build -p forge-cli --locked
```

```sh
cargo build -p forge-cli --locked
```

The executable is under target/debug; release mode uses target/release. Native release artifacts
are unsigned unless a release explicitly reports verified signing. Do not disable operating-system
protections to run an unsigned application. No end-user Rust/Node requirement is claimed for binaries.

## CLI controls

`--help`, `--version`, `--json`, `--repository`, `--offline`, `--non-interactive`.
Preview limitations: profile and timeout flags currently do not change behavior; acquisition uses
fixed bounded limits. Project scope uses an explicit absolute existing destination.

Exit 0: success; 2: review-required/partial/unsupported capability; 3: invalid input/conflict/failure.
JSON output contains only the operation result/error; diagnostics use stderr and per-run files.
`request <file>` submits a structured shared-service request, never a shell command.

## Safe lifecycle

Run install with `--dry-run` first. Files already present without an unchanged ownership receipt
cause a conflict, even if content appears identical. Disable removes only unchanged owned activation
files; enable restores saved bytes only to unchanged absent destinations. Uninstall preserves durable
local history/data. Rollback checks expected current bytes; user edits are never silently replaced.
