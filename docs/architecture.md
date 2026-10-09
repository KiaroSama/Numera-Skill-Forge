# Architecture

Rust core owns registration, discovery, approval, deterministic package builds, journals and receipts.
CLI and Tauri dispatch typed JSON operations into the same application service. The frontend cannot
execute source code or bypass backend path/approval checks; it renders diagnostics as text.

Data flow: registry -> immutable Git blobs -> discovery -> explicit digest approval -> adapter
assessment -> candidate artifact -> journalled promotion -> owned installation receipt.
Observed upstream revisions are local state, accepted snapshots are locked public inputs. Selected
source blobs are read directly, avoiding checkout filters/line-ending transformations.

## Boundaries

- model/validation: component families and portable input constraints.
- source/discovery: bounded acquisition and metadata-only inspection; no upstream execution.
- adapters/build: capability assessment and reproducible artifacts, not host enablement.
- transaction/install: expected-byte comparison, rollback/recovery and owned file lifecycle.
- service: shared user operation boundary; CLI/UI only map requests and display results.
- publish: explicit intended-path publication and remote identity checks.

All text is UTF-8; binary bytes preserved. Real copies are default, no symlink privileges required.
No HTTP management server, telemetry, AI subscription, daemon or GPU runtime is required.

## Development preview limits

This repository is not yet the entire requested finished product. Native runtime component coverage,
full configuration three-way merge/shared dependency ownership, stable-release selection, generic
SSH/custom Git providers, job cancellation/attach, on-open apply-approved rebuild closure, PR
publication and complete native OS/host release smoke certification require further implementation.
Current explicit failures are not advertised as successful support. File-installed skills and native
runtime plugins remain distinct. Executable-mode Git companions are refused in this preview rather
than copied with lost POSIX permissions; full mode-preserving native lifecycle remains unfinished.
New journals store bounded digest metadata plus raw recovery blobs, and ownership receipt changes
participate in the same logical operation. Rollback records its direction before compensation. Existing source licensing and last-known-good state are preserved.

## Registered skill collections

Collection services provide a separate source-to-many-plugins distribution boundary: explicit per-root
license/revision registration, immutable batched Git blobs, unchanged payloads and mode-aware journals.
Accepted state, plugin output and marketplace promotion share logical transactions. New runtime/
license/structure changes are not inferred trust; reviewed prompt body changes alone may advance.
CLI and launcher use the same service; scheduled publication stages only accepted output paths.
This does not broaden existing native installation certification or execute upstream helpers.
