# Collection synchronization and publication

Use the shared Rust pipeline; the launcher and daily workflow never run imported skill scripts,
install host extensions, or rewrite original upstream prompts. Each accepted skill is a separate
Claude Code plugin with its complete licensed companions and recorded Git modes.

## Local commands

PowerShell 7 and Git must already be installed. The launcher resolves its own repository, including
paths with spaces, from any working directory. It prefers `target/release/numera-forge` (on Windows,
`numera-forge.exe`) and falls back to `target/debug`. Nothing is installed implicitly.

```powershell
# Read-only collection check is the default.
& 'G:\path with spaces\Numera Skill Forge\run.ps1'
& 'G:\path with spaces\Numera Skill Forge\run.ps1' -Check

# Explicitly build the trusted Forge CLI using the locked dependencies and two Cargo workers.
& 'G:\path with spaces\Numera Skill Forge\run.ps1' -Check -Build

# Accept only already-reviewed initial pins or permitted safe content updates.
& 'G:\path with spaces\Numera Skill Forge\run.ps1' -Apply
& 'G:\path with spaces\Numera Skill Forge\run.ps1' -Check -Offline
& 'G:\path with spaces\Numera Skill Forge\run.ps1' -Verify
```

These map to `numera-forge --repository <root> --json collection sync`, with `--apply` or
`--offline` when requested, and `collection verify`. Check does not promote public output. Apply
requires the manually reviewed source registration and selected revisions; it is not approval of
arbitrary upstream executable changes. Verify checks the accepted bytes, metadata and native
catalog without running upstream content. Offline sync requires the applicable cached snapshots.

Exit `0` means the operation completed, `2` means review or partial success, and other nonzero
codes mean failure. A partial run may accept healthy independent deltas while retaining the prior
output for failed or review-required entries. The launcher preserves the CLI exit code.

## Automatic update policy

Only unchanged-structure, non-executing prompt-content changes may advance automatically. License,
NOTICE, file topology, executable mode, scripts, binary content, runtime/frontmatter or privilege
changes require a new review. Existing accepted output remains available while review is pending.
Source registration in `registry/collections.json` is reviewed manually, never staged by the daily
publication helper. Acquisition reads Git blobs; upstream build scripts and lifecycle hooks are not
executed. New source approval and host installation are separate actions.

Public outputs are limited to:

- `packages/claude-code/`: separately installable plugins and preserved skill payloads.
- `catalogs/collections.lock.json`: accepted revision, exact file hashes/modes and ownership digests.
- `catalogs/collections.json`: generated collection summary.
- `.claude-plugin/marketplace.json`: native marketplace listing.

## Daily GitHub Action

`.github/workflows/upstream-sync.yml` runs daily at **03:37 UTC** and supports manual dispatch on
the default branch. GitHub schedules are best-effort: runs can be delayed or dropped. Concurrent
collection runs queue instead of canceling a publisher mid-write.

A read-only build job compiles the trusted Forge CLI with two Cargo workers and transfers only that
binary in a one-day artifact. A separate writer job runs sync/apply, validates the resulting
collection, and publishes only exact files named in the accepted lock plus the generated catalogs.
Both jobs have a 20-minute ceiling; sync and publication steps also have explicit bounds. Checkout
does not persist credentials, and only the publication step receives the token in its environment.
The writer job requires `contents: write` for the push and `actions: write` for CI dispatch.

Publication refuses unrelated or pre-staged changes, deletion/rename/conflict states, an unexpected
origin, and any remote head advancement. It hashes raw files without Git filters and stages locked
`100644`/`100755` modes directly, so Windows does not flatten executable metadata or change bytes.
It commits as `github-actions[bot]` and pushes without force. No generated diff means no commit,
push or CI dispatch. Failures preserve outputs and the index for inspection; nothing is reset or
cleaned automatically.

`GITHUB_TOKEN`-created pushes do not trigger push workflows. After a successful push the helper
explicitly dispatches `ci.yml`; it reports the generated SHA, and CI must be checked for that exact
`headSha`. Dispatch success is not a claim that tests passed. A partial sync still publishes any
validated healthy accepted deltas, then finishes nonzero with a review-needed summary. Protected
branches that reject a direct push remain a reported publication failure; no force or protection
bypass is attempted.

## Review and stage locally

```powershell
# Validate only; no index/remote mutation by default.
pwsh -NoProfile -File scripts/publish-collections.ps1

# Explicitly stage exact generated bytes and modes without committing or pushing.
pwsh -NoProfile -File scripts/publish-collections.ps1 -StageOnly
git diff --cached --stat
git diff --cached --summary
```

Run staging only in a clean checkout with generated-only pending changes. The workflow-only
`-Publish` mode additionally requires the expected starting SHA, intended repository/default branch
and GitHub Actions context. Never use it to stage unrelated edits or review source registrations.

## Diagnostic logs

The launcher and publication helper create new UTF-8 files per run under `.forge-state/logs`,
respectively `run_YYYY-MM-DD_HH-mm-ss_UTC.log` and
`publish-collections_YYYY-MM-DD_HH-mm-ss_UTC.log`; collisions receive a numeric suffix without
replacing previous logs. Entries use `[YYYY-MM-DD HH:mm:ss UTC] [LEVEL] [COMPONENT] Message`.
Startup, selected mode, resolved paths, subprocess exit codes, failures, duration and closed cleanup
are logged. Levels are INFO, WARNING and ERROR; Forge's own DEBUG logging uses `NUMERA_DEBUG=1`.
The launcher redacts known credential variables and token/password/secret patterns. The publisher
never records token values, authenticated URLs or raw Git diagnostics; authentication is temporary
process environment state. File-logging failures fall back to console diagnostics. Logs are not
published or attached as workflow artifacts and are retained locally until the owner removes them.
For support, inspect and sanitize the relevant run log before sharing it; never attach credentials
or unreviewed upstream bodies.

Sources: [GitHub workflow triggering](https://docs.github.com/en/actions/how-tos/write-workflows/choose-when-workflows-run/trigger-a-workflow),
[scheduled events](https://docs.github.com/en/actions/reference/workflows-and-actions/events-that-trigger-workflows#schedule),
and [Git index metadata](https://git-scm.com/docs/git-update-index).
