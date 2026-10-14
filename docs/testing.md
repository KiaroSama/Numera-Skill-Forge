# Verification

Core tests exercise public registration, deterministic build, portable paths, owned lifecycle and
interrupted recovery. Fixtures are first-party harmless local Git repositories and isolated project
folders; no user credentials, paid API, upstream scripts or user harness state. The hosted protocol
tier generates ephemeral fixture credentials only, never accesses a user's private service.

CI runs Core/CLI tests on Windows/macOS/Linux, formatting and clippy. Linux runs the same Cargo
selection locked/offline inside the [controlled input boundary](ci-input-contract.md), then exports
the release CLI while its private tmpfs is still mounted. Frontend typecheck/Vite/headless UI run
once in that same prepared image in a separate fresh controlled container, replacing the previous
standalone desktop job. Windows/macOS Core tests retain their native runner paths. Producer unit and
public-CLI tests run on all three platforms; actual Docker denial/cancellation/cleanup tests run on Linux. Native
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

## Collection delivery contracts

`collection_updates.rs` covers native descriptor/catalog coherence, metadata-only legacy migration,
no-op identity preservation, edited-wrapper refusal, safe updates and clean reconstruction.
`shared_updates.rs` verifies that an empty legacy collection remains valid without a new Cursor
catalog and that equivalent destination spellings still produce one coherent installed update.
Nonempty fully migrated collections must still refuse a missing native catalog.
`collection_delivery.rs` parametrizes the verified project/user skill destinations at the shared
service seam, with preview/no-write, exact original/companion bytes, disable/enable, uninstall/edit
refusal, rollback, duplicate-name conflict and accepted installed updates. Native-only profiles refuse.
`verify-collections.py` independently verifies all original bytes/modes plus native descriptor/catalog
membership; it never executes a collected companion. These checks do not certify every original host
runtime or third-party skill tool. See [setup and evidence limits](host-setup.md).

## Evidence status

Initial path test assertion passed, but its guarded run reported a transient child survivor; exact
PID subsequently inspected and already exited. That run is not final green verification. Final
exact-SHA CI evidence and native artifact launch evidence must be checked before completion.
Native host activation and GUI smoke claims require separately recorded exact-SHA evidence. Original
Windows/Linux/macOS ARM/macOS Intel GUI paths have passed in the native tier; all-host activation is
not implied by builds or listed static-contract profiles.

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
and user-edit refusal. The original davila collection matched 4272 original payload/notice files
from 485 plugins against immutable upstream Git identities and modes. Two separately registered
origins add the three-file `clone-website` and seven-file `reverse-engineer-anything` roots with both
complete MIT notices, for 487 plugins and 4284 original files. The independently registered
`apk-reverse` addition retains 114 in-root files plus its original MIT notice, bringing the inventory
to 488 plugins and 4399 original files. Packaging and delivery tests do not run its Android analysis
scripts or certify their runtime behavior. A public-service regression exercises
healthy-origin updates while a failed independent origin retains its accepted bytes and lock entry;
repeating the operation must leave the healthy accepted version unchanged. The five AI Maestro additions
retain the author's concise submitted variants and complete original MIT notices. No source runtime
scripts were run.
Generic build catalog regressions require deterministic sorted entries, preserved native names/optional
versions, portable ownership across a fresh checkout, collection-catalog coexistence, edited/deleted/
unowned output refusal and coherent package/lock/catalog rollback. These are packaging contracts, not
native marketplace installation or model invocation.
Additional public seams cover dependency closure/coherent lock rollback, scoped/shared installation,
managed JSON baselines, durable per-step job progress, failed-only retry (completed observations are
not rerun; review-only work cannot bypass approval), cancellation boundaries and local support privacy.
Headless UI cases cover check-only versus explicitly opted-in workspace reopening, explicit source
policy/enabled choice and confirmation/refusal, and progress/cancel controls while a source operation
is busy. Mock IPC proves UI contracts only. Recursive inventory/build fixtures require exact nested
agent and Gemini command paths, separate same-basename siblings and unconverted owning-skill companions;
no upstream code runs.
CI `scripts/verify-collections.py` verifies copied raw bytes, staged metadata, complete selected roots
and native marketplace entries. This complements native `claude plugin validate`, not host load proof.

`scripts/check-launchers.ps1` is an isolated first-party Git fixture: alternate CWD, launcher apply/
verify, unrelated-change refusal, raw blob/mode staging and no-op. It never pushes, invokes imported
scripts or installs extensions. CI owns the three-OS final pass; three-minute bound, no parallel shared
fixture. Binary assets remain exact original bytes and are validated separately from UTF-8 project text.

`tests/publication.rs` exercises changed-content commit/push against an owned local bare repository,
including exact raw CRLF/modes, user staging preservation, remote/head advance refusal, deletion and
no-op. Committed accepted-original instructional companions are distinguished from private files
using the committed registry, lock, exact source/output mapping, digest-verified original skill name
and actual committed blob bytes. Working-tree metadata cannot self-approve an added or edited private
instruction file; secret/configuration boundaries remain absolute. Baseline blobs are read in bounded
batches, not one Git process per collection. This is real local Git publication, not proof of
authenticated GitHub PR creation. On 2026-10-13 the actual application publisher also created
[the additional-collections PR](https://github.com/KiaroSama/Numera-Skill-Forge/pull/2) from meaningful
reviewed package data, with independently verified repository/base/head identity and clean owned
process cleanup. The integrated branch's exact-SHA CI, not PR creation alone, supplies final checks.

The manual native workflow builds four unsigned lanes: Windows x64, Linux x64, macOS ARM and
macOS Intel. `scripts/distribution-manifest.py` records SHA-256 per bundle file, exact workflow commit,
runner architecture and locked Cargo license inventory; missing signing remains explicitly unsigned.
Manifest generation logs UTC UTF-8 under project-local diagnostics; those logs are not public bundles. Its Linux tier runs the original
release through tauri-driver/WebKitWebDriver under Xvfb using `scripts/native-desktop-smoke.py`:
real status/sync IPC, source-policy form persistence/consent reset and unknown-source refusal without
approval, invalid workspace, diagnostic file and owned-process cleanup.

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
disabled.

macOS ARM/Intel lanes use an external Apple Accessibility client against the unchanged production
release, not an embedded WebDriver test build. `native-macos-smoke.py` owns the app and existing
`/usr/bin/osascript` client; `native-macos-ax.js` targets only that PID's windows. Hosted-only checks
require existing nonprompting AX trust and real semantic control exposure; no TCC/permission writes,
application activation/global keystrokes, coordinate fallback, injected app JavaScript or paid driver.
Text controls enter editing mode through their own checked AXFocused attribute before a single value
write; this targets only the owned control on the remote ephemeral runner, not the user's desktop.
Invalid elements during read-only snapshots restart under the existing readiness deadline; actions
are never replayed and persistent or unsupported errors remain failures. Actual UI status,
check-only job, policy persistence/consent reset/refusal, invalid workspace and dispatch logs must
agree. Missing prerequisites fail explicitly rather than skip green. The 120-second wall bound,
10-second helper calls, one-second AX messaging timeout, bounded tree/output and PID/start-identity
cleanup guard the scenario. Capture UTC UTF-8 diagnostics under the project work directory; upload
only sanitized log files, never scratch configuration or private state. Correlate results with the
exact workflow SHA and runner architecture before claiming macOS GUI success.

Current acceptance excludes private-repository/account tests and model invocation by owner choice.
Real authenticated first-party transport fixtures remain valid, but are not private-account evidence.
All-host non-model activation needs each host's actual isolated supported entry point; metadata
validation and model-facing skill invocation are not substitutes.

## Isolated non-model host tier

Hosted Linux builds `scripts/host-activation.Dockerfile` from the restricted `scripts/` context and
pinned official Node/Pi/Gemini distributions. Installation is separate from acceptance. The runtime
never pulls packages, receives a Docker socket, credentials or a personal HOME. The controller verifies
image identity and container flags; the original API probe independently verifies empty allowlisted
environment, denied network/runtime writes and actual cgroup/tmpfs limits before host imports.

Pi uses its original resource loader and in-memory session to dispatch one registered command before
model validation; exact nonce/body digest, handler error, unknown command and missing extension are
load-bearing assertions. Gemini uses its actual Config/SkillManager/ActivateSkillTool without client
initialization/authentication; exact instruction body and active state, malformed/unknown/empty and
disappeared-skill cases must agree. Direct core execution does not test interactive confirmation.

A 120-second owner wall ceiling, 20-second idle limit, 1 MiB output bound, two CPUs, 1 GiB memory/no
swap, 64 PIDs, 128 MiB scratch and five-second cleanup operations bound the tier. The controller owns
one uniquely labelled container and verifies its absence in finally, including failure. Only sanitized
UTC UTF-8 logs and the first-party result record are uploaded. Native API evidence is not all-host,
normal CLI, collected-skill execution or model efficacy. Missing isolation or native dependencies fail;
no patched/mocked host fallback or green skip. Image setup runs a full npm audit on the Gemini lock;
any finding fails before runtime acceptance. Compatible official dependency overrides remove known
advisories, including the old archive dependency, without patching Gemini's source. YAML 4.x schema
and legacy CommonJS UUID exports remain preserved. The final exact-SHA CI supplies runtime evidence.

The original macOS helper observes the requested popup label after its sole menu selection, just as
text setters observe their values. Readback is not proof that DOM events have drained. Bounded
policy-ID/selection/enabled/consent snapshots precede Save; cleared consent fails closed, never gets
silently rechecked. Timeout logs retain first-party UI notice/error text. These observations preserve
the original required-consent, real IPC/persistence and failure assertions; no action replay or new
permission is introduced.

## Historical configuration evidence

Two genuine pre-update registration backups matched their current files byte-for-byte. Historical
engine configuration and environment settings without an original backup remain nonrecoverable;
a newly created baseline cannot prove their past preservation. The first four-file comparison on
2026-10-13 matched all SHA-256 digests, solely as future-change evidence. A later comparison found
concurrent changes in two registration files while the other two remained unchanged. The actor and
origin of that change were not verified; current files, genuine backups and the earlier baseline were
preserved without restoration. Final unchanged-configuration preservation is not claimed.
No past configuration or test receipt was reconstructed. A stale observing-hook run must be recovered
by an actual fresh execution of the same complete command and independently matched observation;
a successful CI run or a different command does not repair that correlation. On 2026-10-13 the exact
previous graph command was rerun with an explicit shared run identity: it completed with exit 0 in
4.8 seconds, no terminated/leaked descendants, and an independently paired observing-hook result.
Historical receipts were preserved unchanged; the fresh run, not a rewritten past verdict, is the
recovery evidence.

## K-Dense collection verification

The complete K-Dense source adds177 independent original packages and2202 payload/notice records;
combined inventory is665 packages and6601 original files. The independent verifier checks Git
membership/modes, exact accepted digests and native catalogs without importing scientific code.
`collection_documents.rs` covers exact reviewed public reference admission, unchanged repeat,
approval withdrawal/hash changes and forbidden root/parent paths. Original dependencies and
scientific/clinical execution are not tested by packaging or distribution fixtures.
