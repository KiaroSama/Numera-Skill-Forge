# Controlled Linux verification

The prospective producer covers Linux Core/CLI tests and release builds, and frontend typecheck/Vite/Playwright. It does not certify native bundles, Windows/macOS, original host activation, private transport, or historical CI carryover. No hook admission or success-cache consumer is included.

## Boundary and prerequisites

Use the existing Linux Docker daemon with an immutable prepared image ID. The daemon, kernel, OCI runtime, compiler, browser, dependency provisioning and supervisor are an explicit trusted base. This is not universal hermeticity or a syscall read-set certificate. User-namespace/Landlock isolation is **not claimed**. No host-root/home/socket bind, privileged container, capability addition or host security-policy changes are allowed. Missing actual controls fail closed; no host fallback. Local WSL bubblewrap0.11.1 is ineligible and is never used.

Source projection uses exact Git objects, declared paths and original modes, not a mutable checkout or ignored content.
The root Cargo manifests/lock and LICENSE, crate sources/tests/fixtures and selected desktop inputs are
explicitly declared. LICENSE is a compile-time input of the GPL distribution regression; it is not
recovered from an ambient checkout. Runtime and prepared dependencies are declared by the entire immutable final image content identity. Dependencies are provisioned before controlled execution; Cargo runs locked/offline. Source is read-only. Only private scratch, home, temp and explicitly named fixture/output subtrees are writable. Container networking has loopback only, for Playwright's server; external networking is disabled.

The supervisor independently inspects nonroot UID, read-only root, no-new-privileges, all capabilities dropped, default seccomp, no host namespace sharing, no host mounts/devices/ports, network none and resource settings **before** starting product commands. Real denial probes test source writes, outside paths, ambient environment/descriptors and external network.

## Budgets

Two CPU/build/test workers maximum; 4GiB memory and equal memory+swap, 256 PIDs. Core wall480s/idle120s; frontend wall300s/idle45s; probe wall60s/idle10s. Source manifest max10,000 files/64MiB. Captured command output max16MiB; evidence max1MiB. Timeouts are failures, not successful skips. Every container is owned by a unique run label and exact ID, removed and absence verified on normal/error/timeout paths. Provisioning is separately bounded and never presented as controlled product execution.

## Evidence and logs

One create-only UTF-8 JSON execution record binds source, contract, manifest, image, job, commands and actual outcome. Success requires verified controls, actual successful commands, input integrity and cleanup. Refused, failed or terminated records cannot certify unperformed work. Atomic publication refuses existing destinations. Raw child output, environment, credentials and host-private paths are not uploaded; bounded counts/digests and stable reason codes are recorded.

Diagnostic logs use unique `forge-ci-input_YYYY-MM-DD_HH-mm-ss_UTC.log` names (a numeric suffix handles same-second collisions) and `[timestamp] [LEVEL] [COMPONENT] Message` entries beside owned run output, with safe console fallback if initialization fails. Retain failed-run diagnostics; CI artifact retention is finite. Attach only the sanitized JSON/log pair for support. Retry creates a new owned run, never overwrites old evidence or relabels historical results.

Run `bash scripts/ci-input.sh` only in the documented Linux CI environment after building
`forge-ci-input --release`. The script projects the exact committed revision and prepares one image,
then executes fresh probe/Core/frontend containers and negative controls. Internal `timeout-probe`
and `failure-probe` modes exercise termination and attempted-command evidence, never arbitrary commands.
Core export occurs before the private scratch tmpfs is unmounted. Failure records preserve the image-bound
manifest digest; completed failed commands retain argv, exit and output digests. No global admission
state or historical result is changed. CI artifacts retain sanitized JSON/logs for seven days;
provisioning captures and arbitrary product output remain outside uploaded artifacts.

Implementation checks are not final runtime acceptance; require the exact revision's Linux CI outcome.
