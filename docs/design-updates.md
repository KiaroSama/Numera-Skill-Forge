# Reviewed design updates

VoltAgent packages contain a Forge-generated skill entry and unchanged original design documents.
Their source document hashes are approvals, not permanent permission to import future instructions.
Ordinary collection sync holds changed designs as `review-required`; it retains the accepted package.
No source scripts, Git diff drivers or text-conversion helpers execute during review.

## Discover and review

Use the Forge checkout and CLI with an explicit repository. Examples below assume the current directory
is that checkout; use the installed `numera-forge` executable or the project's documented source route.

```bash
numera-forge --repository . --json collection design-list voltagent-design
```

The inventory reports `registered`, `new` and `missing` direct `design-md/<name>` roots. A new root
is not registered automatically, and a missing root never deletes a package or installation.

```bash
numera-forge --repository . --json collection design-review voltagent-design design-md/voltagent
```

Review the complete returned `documents`, original source `diff`, proposed `registration`, file modes,
license/notices, package version and generated ownership hashes. This is untrusted source material,
not commands to execute. The result's `revision` is immutable; `digest` binds that candidate to the
current registry and accepted collection state. Review may populate the inert Git cache but does not
promote a registration, package, lock or catalog. Reviews larger than 2 MiB refuse rather than truncate.
Use `--offline` only when the required source revisions are already cached.

## Accept exactly what was reviewed

Substitute the exact values returned above; the placeholders below are not real approvals.

```bash
numera-forge --repository . --json collection design-accept voltagent-design design-md/voltagent --revision REVIEWED_COMMIT_SHA --digest REVIEWED_CANDIDATE_SHA256
```

Acceptance recomputes the pinned candidate. A wrong digest, changed registry/accepted state, edited
package/catalog, missing original, unsupported adapter or invalid license evidence refuses. Review
again rather than changing hashes manually. Old content stays usable on refusal.

One recoverable transaction updates the selected registration, original notices, package, accepted
lock and native catalogs, including removal of obsolete owned companions. Existing package/skill names
stay stable; a changed accepted package advances its patch version. A new root derives a deterministic
name and refuses collisions. Repeating the exact successful acceptance is `unchanged`; a fresh review
of unchanged content also avoids version churn. Original root license/disclaimer changes are included
in approval, not silently inherited. This bounded route accepts regular UTF-8 design documents only;
it does not import executable/binary companions or replace an upstream-native `SKILL.md`.

```bash
numera-forge --repository . --json collection verify
```

Commit/review and publish accepted outputs using the repository's normal gates. Acceptance itself
never commits, pushes, changes an account, grants permissions or activates a personal host profile.
The CLI writes its existing per-run UTF-8 diagnostic logs; JSON stdout contains the result, not logs.

## Update consumers separately

For installations owned by Forge:

```bash
numera-forge --repository . --json update
```

The existing shared receipt engine updates installed bytes only when ownership and current-content
checks pass. User edits are preserved and cause refusal; unrelated consumer files remain untouched.
Native plugin-manager installations use their host's documented marketplace refresh/plugin-update
route after publication instead. See [host setup](host-setup.md). Package acceptance is not proof of
model-followed visual quality, installation on every host or a signed application release.
