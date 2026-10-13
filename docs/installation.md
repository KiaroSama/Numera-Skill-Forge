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
The manual native workflow also supplies `SHA256SUMS.txt` and `distribution-manifest.json` with exact
commit, native runner architecture, package-file hashes and locked Cargo dependency license inventory.
These are integrity/provenance records, not signatures or proof that every host runtime was exercised.
macOS artifacts contain the native `.app` bundle for Intel or ARM; the manual workflow does not promise
an unsigned DMG installer. Locked metadata resolution may download missing Cargo manifests, never
runs dependency builds or changes the lockfile, and remains bounded.

## CLI controls

`--help`, `--version`, `--json`, `--repository`, `--offline`, `--non-interactive`.
`--profile` selects an isolated named workspace beneath the selected repository's local state.
`--timeout` bounds job-enabled discovery/build/sync/update operations (1-3600 seconds); cancellation
stops at safe checkpoints and never abandons an applying transaction. `job run <request.json>`
accepts an explicit `--id`, so a second CLI can inspect/cancel that job. `job list/status/cancel/retry`
retain durable terminal/error evidence. Sync jobs persist per-source/build/collection/receipt progress;
retry selects failed/running/pending steps and does not repeat completed sources. Review-required
content is not automatically retried or approved. Legacy records without progress restart the original
bounded request. Project and user scopes require explicit existing destinations.

Desktop Sources provides explicit per-source update policy and enabled-state controls. Review the
current values, authorize the selected changes, then save; the confirmation resets after success or
any choice change. A failed save remains a diagnostic, not source/executable approval.

Desktop Settings offers a per-workspace opt-in to apply policy-approved safe updates and rebuild on
open. Check-only remains the default; no installation or publication runs on open. Active progress
inspection and cancellation remain available while an operation runs. After reopen, Updates accepts a
persisted job ID for status/cancel/retry; inspection does not run the job or poll in the background.

Exit 0: success; 2: review-required/partial/unsupported capability; 3: invalid input/conflict/failure.
JSON output contains only the operation result/error; diagnostics use stderr and per-run files.
`request <file>` submits a structured shared-service request, never a shell command.

Publication defaults to preview. Mutation requires exact `--expected-head`, public author name/email;
remote modes additionally require `--expected-remote`, `--expected-remote-head` and
`--expected-account`. Pull-request mode requires an explicit `chore/` publication branch. The
publisher stages only intended distribution paths in an isolated index, preserves unrelated user
staging, hashes raw bytes without upstream filters and never force-pushes. Real account/protection/
CI requirements remain separate checks; a local fixture does not certify them.

## Explicit source authentication

Supply only public environment-key names in source registration. HTTPS `credential_reference` resolves
an explicitly supplied Basic/Bearer authorization value into the isolated Git child, not command-line
arguments, configuration files or logs. SSH requires explicit public-key, pinned-host and agent references;
private keys and personal host configurations are not imported. See [source policies](source-format.md).
Configure credentials through an approved local/deployment secret manager; never put real values in
examples, shell history, public registration or support bundles. Private service connectivity still
requires an authorized endpoint and runtime verification; input validation is not an end-to-end claim.

## Host-specific setup

[Native catalogs and standalone skill installation](host-setup.md) covers every documented skill host,
including Antigravity desktop, OpenCode v1/v2 and Hermes. Accepted collection IDs are available through
catalog/inspect/build/compatibility and scoped installs using their shared original payload, without
registering duplicate generic packages. Manager-installed native plugins and Forge-owned standalone
receipts have different update/removal owners; the guide keeps them separate.

## Safe lifecycle

Run install with `--dry-run` first. Files already present without an unchanged ownership receipt
cause a conflict, even if content appears identical. Identical files already owned by another Forge
receipt can be shared; uninstall/disable retains files still needed by an enabled owner. Disable removes only unchanged owned activation
files; enable restores saved bytes only to unchanged absent destinations. Uninstall preserves durable
local history/data. Rollback checks expected current bytes; user edits are never silently replaced.
`update --installed` builds the accepted packages and updates all enabled owners per destination
coherently, including shared configuration baselines and every participating receipt. Failed independent
destinations report partial success; disabled owners still reserve their intended content. Conflicting
next shared bytes/modes/fields block the destination before any installed write.
