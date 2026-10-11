# Verification

Core tests exercise public registration, deterministic build, portable paths, owned lifecycle and
interrupted recovery. Fixtures are first-party harmless local Git repositories and isolated project
folders; no user credentials, paid API, upstream scripts or user harness state. The hosted protocol
tier generates ephemeral fixture credentials only, never accesses a user's private service.

CI runs `cargo test --locked -p forge-core -p forge-cli -- --test-threads=2` on Windows/macOS/Linux,
formatting and clippy. Desktop typecheck/build and headless UI contracts are a separate tier. Native
build success and browser UI tests are not proof of real-host loading or packaged GUI startup.

Every suite is discovered by Cargo or the frontend runner, not a hand-maintained test file list.
Historical-state fixtures under `crates/forge-core/tests/fixtures/state-compatibility` model the actual
first-release v1 registry/lock/receipt and pre-progress job formats. Shared-service regressions require
logical state and receipt identity preservation through current lifecycle, user-edit refusal, legacy
job retry, and unsupported-version refusal without rewriting the selected file. These are harmless
structural fixtures, not execution of an old binary or a claim of arbitrary-version migration.
Commands/job timeouts and two-worker ceilings bound CI. Local light tests use the installed guarded
runner with wall/idle bounds, fresh fingerprints and process-tree cleanup. Full final checks run
once in CI on the pushed SHA, not duplicated locally.

## Evidence status

Initial path test assertion passed, but its guarded run reported a transient child survivor; exact
PID subsequently inspected and already exited. That run is not final green verification. Final
exact-SHA CI evidence and native artifact launch evidence must be checked before completion.
All real native harness load tests and cross-platform packaged GUI smoke paths remain unverified
until explicitly recorded; a listed profile is static contract evidence only.

## Authenticated source protocol tier

The hosted Linux CI tier runs `scripts/verify-source-transport.py` inside new private mount, network
and PID namespaces. Only loopback is enabled. A temporary CA bundle is mounted privately for actual
HTTPS certificate verification; production TLS defaults are unchanged. Git's real smart HTTP backend
requires an explicit fixture authorization value, refuses redirects and never enables receive-pack.
OpenSSH uses one owned temporary agent identity and pinned host key, forced upload-pack and no
password/forwarding/user-rc path. The actual release CLI performs discovery, approval, build, safe
update and offline/failed-update retention; unauthorized and mismatched-key cases must refuse.

The tier has a 150-second internal/180-second outer wall ceiling, 20-second child idle limit, bounded
output and owned process-group cleanup. An incomplete TLS handshake must time out before the HTTP
handler and permit subsequent authenticated requests. Strict SSH key-path ownership is retained through
a namespace-private root-home mount; the hosted checkout's ownership is never changed.
Fixture credentials, certificates, agent socket and repositories
are removed in finally; only sanitized UTC logs are uploaded. Missing isolation/prerequisites fail,
not skip green. Synthetic first-party repository structure gives real protocol coverage, not evidence
of access to an operator's private repository. Held license/compliance cases are excluded from the
current task by the owner and never count as accepted payloads.

## Skill collection contracts

Collection fixtures exercise exact original companion scripts without execution, metadata modes,
idempotence, safe prompt updates versus script/license/topology review, clean-workspace reconstruction
and user-edit refusal. The assembled reviewed source import matched 4272 original payload/notice files
from 485 plugins against immutable upstream Git identities and modes. The five AI Maestro additions
retain the author's concise submitted variants and complete original MIT notices. No source runtime
scripts were run.
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
real status/sync IPC, invalid workspace, diagnostic file and owned-process cleanup.

Windows uses the original production release through the operating system's UI Automation provider,
not a debug build, injected bridge, browser mock or enabled production debugger. An OS-provided C#
compiler builds `scripts/native-windows-uia.cs` against existing UI Automation assemblies. Its
ValuePattern/InvokePattern controls enter the workspace and navigate Updates; TextPattern reads the
rendered response. Assertions require zero source/package/receipt counts, the exact workspace,
successful empty-source sync, the real invalid-workspace error, one persisted check-only job and
actual desktop dispatch logs. No focus, keyboard injection or user desktop switch is used.

The `pythonw.exe` worker and all native children run on one never-switched private desktop inside a
kill-on-close Job. Elevated hosted runners use a linked or same-user/session ordinary token when
available, otherwise a LUA-filtered Medium token. Filtering removes administrator rights without
stripping normal-user capabilities. Only the new child token's default DACL is normalized to retain
its user's access to newly-created objects; existing entries remain intact. No elevated parent,
filesystem, account, UAC or machine policy is modified. Candidate and actual suspended child must be
non-elevated, at Medium or lower integrity, and without an enabled Administrators SID before resume.
A single inherited private desktop handle keeps downstream native children off the input desktop.

A 15-second first-party launch preflight fails early before native compilation. The actual UI tier
has a 120-second protocol/150-second owner/40-second progress ceiling, with bounded child cleanup
and a verified zero-active-process Job before desktop closure. Disappearing descendant files during
Python 3.12 profile cleanup are treated like Python 3.13 `rmtree`. After zero owned processes,
sharing-violation 32 alone may wait within one five-second fixture-cleanup deadline; access denial,
other errors and exhausted waits propagate. No external process is terminated or permission changed,
and every fixture root must be absent. A Windows-only diagnostic selection avoids unrelated rebuilds; the
workflow's default `all` selection retains four-platform final verification. Missing native runtime,
UI Automation, token, desktop or cleanup capabilities fail rather than skip green.

Windows original-release native interactions and cleanup have passed in the hosted tier; correlate
published results to their exact SHA before claiming final completion. Production devtools remain
disabled. macOS GUI startup remains untested without a supported isolated silent native test provider.
