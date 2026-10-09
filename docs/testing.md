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
and user-edit refusal. The real approved source import independently matched2082payload blobs from65
plugins against upstream Git SHA-1;80executable Git modes recorded. No source runtime tests were run.
CI `scripts/verify-collections.py` verifies copied raw bytes, staged metadata, complete selected roots
and native marketplace entries. This complements native `claude plugin validate`, not host load proof.

`scripts/check-launchers.ps1` is an isolated first-party Git fixture: alternate CWD, launcher apply/
verify, unrelated-change refusal, raw blob/mode staging and no-op. It never pushes, invokes imported
scripts or installs extensions. CI owns the three-OS final pass; three-minute bound, no parallel shared
fixture. Binary assets remain exact original bytes and are validated separately from UTF-8 project text.
