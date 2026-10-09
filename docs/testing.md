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
