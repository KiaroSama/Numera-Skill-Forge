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
`--profile` selects an isolated named workspace beneath the selected repository's local state.
`--timeout` bounds job-enabled discovery/build/sync/update operations (1-3600 seconds); cancellation
stops at safe checkpoints and never abandons an applying transaction. `job run <request.json>`
accepts an explicit `--id`, so a second CLI can inspect/cancel that job. `job list/status/cancel/retry`
retain durable terminal/error evidence. Project and user scopes require explicit existing destinations.

Exit 0: success; 2: review-required/partial/unsupported capability; 3: invalid input/conflict/failure.
JSON output contains only the operation result/error; diagnostics use stderr and per-run files.
`request <file>` submits a structured shared-service request, never a shell command.

Publication defaults to preview. Mutation requires exact `--expected-head`, public author name/email;
remote modes additionally require `--expected-remote`, `--expected-remote-head` and
`--expected-account`. Pull-request mode requires an explicit `chore/` publication branch. The
publisher stages only intended distribution paths in an isolated index, preserves unrelated user
staging, hashes raw bytes without upstream filters and never force-pushes. Real account/protection/
CI requirements remain separate checks; a local fixture does not certify them.

## Safe lifecycle

Run install with `--dry-run` first. Files already present without an unchanged ownership receipt
cause a conflict, even if content appears identical. Identical files already owned by another Forge
receipt can be shared; uninstall/disable retains files still needed by an enabled owner. Disable removes only unchanged owned activation
files; enable restores saved bytes only to unchanged absent destinations. Uninstall preserves durable
local history/data. Rollback checks expected current bytes; user edits are never silently replaced.
