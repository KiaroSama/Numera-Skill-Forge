# Verification

Core tests exercise public registration, deterministic build, portable paths, owned lifecycle and
interrupted recovery. Fixtures are first-party harmless local Git repositories and isolated project
folders; no credentials, paid API, upstream scripts or user harness state.

CI runs `cargo test --locked -p forge-core -p forge-cli -- --test-threads=2` on Windows/macOS/Linux,
formatting and clippy. Desktop typecheck/build and headless UI contracts are a separate tier. Native
build success and browser UI tests are not proof of real-host loading or packaged GUI startup.

Every suite is discovered by Cargo or the frontend runner, not a hand-maintained test file list.
Commands/job timeouts and two-worker ceilings bound CI. Local light tests use the installed guarded
runner with wall/idle bounds, fresh fingerprints and process-tree cleanup. Full final checks run
once in CI on the pushed SHA, not duplicated locally.

## Evidence status

Initial path test assertion passed, but its guarded run reported a transient child survivor; exact
PID subsequently inspected and already exited. That run is not final green verification. Final
exact-SHA CI evidence and native artifact launch evidence must be checked before completion.
All real native harness load tests and cross-platform packaged GUI smoke paths remain unverified
until explicitly recorded; a listed profile is static contract evidence only.

## Skill collection contracts

Collection fixtures exercise exact original companion scripts without execution, metadata modes,
idempotence, safe prompt updates versus script/license/topology review, clean-workspace reconstruction
and user-edit refusal. The assembled reviewed source import independently matched4262payload blobs from480
plugins against immutable upstream Git SHA-1 and mode records. No source runtime scripts were run.
Additional public seams cover dependency closure/coherent lock rollback, scoped/shared installation,
managed JSON baselines, durable per-step job progress, failed-only retry (completed observations are
not rerun; review-only work cannot bypass approval), cancellation boundaries and local support privacy.
Headless UI cases cover check-only versus explicitly opted-in workspace reopening and progress/cancel
controls while a source operation is busy. Mock IPC proves UI contracts only.
CI `scripts/verify-collections.py` verifies copied raw bytes, staged metadata, complete selected roots
and native marketplace entries. This complements native `claude plugin validate`, not host load proof.

`scripts/check-launchers.ps1` is an isolated first-party Git fixture: alternate CWD, launcher apply/
verify, unrelated-change refusal, raw blob/mode staging and no-op. It never pushes, invokes imported
scripts or installs extensions. CI owns the three-OS final pass; three-minute bound, no parallel shared
fixture. Binary assets remain exact original bytes and are validated separately from UTF-8 project text.

`tests/publication.rs` exercises changed-content commit/push against an owned local bare repository,
including exact raw CRLF/modes, user staging preservation, remote/head advance refusal, deletion and
no-op. This is real local Git publication, not proof of authenticated GitHub PR creation.

The manual native workflow builds four unsigned lanes: Windows x64, Linux x64, macOS ARM and
macOS Intel. `scripts/distribution-manifest.py` records SHA-256 per bundle file, exact workflow commit,
runner architecture and locked Cargo license inventory; missing signing remains explicitly unsigned.
Manifest generation logs UTC UTF-8 under project-local diagnostics; those logs are not public bundles. Its Linux tier runs the original
release through tauri-driver/WebKitWebDriver under Xvfb using `scripts/native-desktop-smoke.py`:
real status/sync IPC, invalid workspace, diagnostic file and owned-process cleanup. Windows now has
an explicit hosted tier using an exact matching Microsoft driver, never-switched private desktop,
linked unelevated token where needed and suspended child attached to a kill-on-close Job before resume.
It asserts original-release DOM/IPC, persisted check-only job, invalid workspace, logs and owned cleanup.
Missing token, desktop, matching runtime or cleanup capability fails, never skips green. Record actual
results per final SHA; a prepared smoke script is not a passing runtime result. macOS GUI startup
remains untested without a supported isolated silent native WebDriver.
