# Source and Package Registration

New content for an existing supported profile is data, not engine code. The active initial registry
starts empty; the owner-approved davila7 collection is registered separately in `registry/collections.json`. Examples never become approved upstreams automatically.

1. Register source with a unique lowercase ASCII ID, credential-free approved HTTPS URL or explicit
   `local:` absolute trusted Git repository path, selected roots and targets.
2. Discover metadata and companion files without running source scripts.
3. Review license, component inventory and candidate content digest.
4. Explicitly approve the exact digest; executable content requires separate trust approval.
5. Register an independently installable package, then build its target with accepted locked inputs.

See [example package record](../examples/package.json), [complete first-party fixture](../examples/complete-inventory/README.md)
and [editor schema](../schemas/registry.schema.json). Components omitted/empty selects discovered
components; required unsupported conversions fail instead of dropping material. Dependencies must
reference existing packages and cannot form cycles.

Registry schema v1: `schema_version`, ordered `sources`, `packages`. Source fields: `id`, `repository`,
`enabled`, `reference` (strategy/default-branch, branch/commit/tag with value), `roots`, `excludes`,
`targets`, `update`, `credential_reference`, preserved metadata. Sources default disabled/review.

Standalone component categories are discovered recursively. Native category-relative directories are
retained when packaging, so two agents with the same filename in different subfolders do not collide;
Gemini nested TOML commands retain their namespace. A skill or complete native plugin still owns its
companions; those are not converted into additional standalone components. A preserved nested path
is not proof that every host automatically loads it; native identity and supported format gates remain.

Desktop Sources exposes `review`, `approved-content-only` and `pinned` update policies plus enabled
state. Inspect the current source response, select the intended values and explicitly authorize the
save. Sources still start disabled/review. Saving policy never approves a candidate digest, grants
executable trust, installs content or publishes. Safe-content acceptance retains executable/license/
structure review gates, even when workspace-open apply is separately enabled.

## Reviewed acquisition providers

Built-in public HTTPS hosts are `github.com`, `gitlab.com`, `bitbucket.org` and `codeberg.org`.
Other public HTTPS repositories require an explicit policy in the source record, reviewed by the
operator (the marker is a decision record, not an automatic security certification).

```json
{
  "id": "reviewed-source",
  "repository": "https://forge.example.invalid/team/skills.git",
  "metadata": {
    "provider_policy": {"transport": "https", "host": "forge.example.invalid", "reviewed": true}
  }
}
```

The policy binds the exact lowercase DNS host and transport; unknown policy fields, wildcard hosts,
userinfo, ports, URL-encoded syntax, traversal, query/fragment and control characters are rejected.
HTTPS uses isolated empty credential/config home, no credential helpers, redirects or checkout;
TLS verification stays enabled. Child transport starts with an allowlisted OS environment; inherited
Git tracing, configuration injection, proxy and implicit authentication variables are not forwarded.
Cached repository configuration is checked before use; extra includes, alternate object stores or
unreviewed settings return `cache-policy` without changing accepted snapshots. The existing process
owner supplies deadlines, cancellation and pipe cleanup; upstream blobs/hooks are never executed.

`credential_reference` is only a public key name matching `[A-Za-z][A-Za-z0-9_-]{0,63}`; no token,
URL, file path or secret-store instruction belongs in the registry. Registration does not read its
value. Online acquisition reads only that caller-provided environment key, containing `Basic <encoded>`
or `Bearer <token>` authorization. The URL-scoped header goes into the isolated child environment via
Git `--config-env`, never argv, repository config, temporary files or logs. Missing/malformed input
returns `credential-missing`/`credential-format`. No ambient credential store/helper is consulted.
Environment memory remains visible to the owning OS user/administrator; this is not a secret vault.
Existing cached snapshots remain usable offline without resolving the key.

Reviewed SSH registration uses `ssh://git@forge.example.invalid/team/skills.git` and the same exact
policy with `transport: "ssh"`. Supply `metadata.ssh_authentication` with public environment key
references `public_key_reference`, `known_hosts_reference`, `agent_reference`. The first two resolve
to explicit absolute, regular public-key/known-host files; the third to an absolute agent socket/pipe.
Known hosts must contain only exact selected host entries, no wildcards or discovery. Forge copies
public evidence into its temporary isolated context and invokes OpenSSH with an empty config,
selected public identity, explicit agent, strict pinned host checking and noninteractive authentication.
No private key, default host/config file, forwarded agent or interactive password is read. Missing
policy refuses acquisition; SCP shorthand and ambient identity fallback remain unsupported.
Real private-host/agent integration requires the operator's authorized accessible endpoint and keys;
static policy validation alone is not proof that that endpoint was tested.

The existing GitHub `latest-stable-release` metadata lookup remains a separate `gh api` helper,
with the CLI's ambient authentication/configuration behavior; it does not resolve source
`credential_reference` or authenticate private clones. Custom-host release APIs are unsupported.
Actual custom HTTPS/private/SSH acquisition has not been integration-tested without an approved
reachable endpoint and explicitly supplied credentials/agent/pinned host evidence. The isolated
transport and input contracts are implemented and tested; those local fixtures do not certify service
connectivity.

Package fields: `id`, `name`, `source`, `components`, `dependencies`, `targets`, `optional_exclusions`.
Unknown host APIs require a capability/adapter change and regression tests; adding more content to an
existing valid profile does not. Unknown/missing licenses block redistribution pending review.

## Built package catalogs

Generic build responses include generated catalog paths. Every target gets `catalogs/builds/<target>.json`
and a matching `.lock.json` ownership record. Claude builds additionally generate
`packages/claude-code/.claude-plugin/marketplace.json`; its plugin sources are `./<package-id>` relative
to `packages/claude-code`. Entry names/versions follow each real native manifest, not invented aliases.
Keep those public ownership records with the outputs. Build/lock/catalog rollback is one operation;
user edits and deleted owned catalog files require review. No automatic host registration is performed.
The repository-root marketplace remains dedicated to exact skill collections below.

## Exact skill collections

`registry/collections.json` records explicitly reviewed skill roots, immutable initial revision and
license path/digest. Original grants omitted by the aggregator can be supplied as `notices` with
tracked original legal-text path, exact digest and immutable source URL; adaptation attribution stays
outside the unchanged skill payload. MIT, Apache-2.0 and CC-BY-4.0 remain distinct grants. Exact
reviewed public environment templates use `reviewed_templates`; real secrets remain forbidden.
See [collection schema](../schemas/collections.schema.json). Each skill gets one plugin. `collection sync --apply` retains complete original
files and executable modes; `collection verify` checks accepted payloads and native catalog without
execution. Adding a reviewed collection or skill is registration data, not an engine edit.
See [licensing](source-licenses.md) and [manual/local updates](upstream-sync.md).

## Reviewed original reference documents

Collection registrations may include an optional `reviewed_documents` map from one exact source
reference path to its SHA-256 digest after explicit content review. Only a canonical
`skills/<name>/references/secrets.md` or
`cli-tool/components/skills/<category>/<name>/references/secrets.md` educational document under an
unprotected skill root qualifies;
root confidential registries, protected parent directories and credential-bearing content remain
excluded. This is not a filename-wide allowlist. Changed bytes, missing content or withdrawn approval
refuse verification/publication of that candidate; last-known-good accepted content remains.
The field defaults empty and is omitted when empty, preserving existing registration metadata.
Original `.env.example` template review remains a separate unchanged contract.

## Reviewed name-only adaptations

An optional `reviewed_name` object records `source_name`, portable `name`, and the complete original
entry `source_digest`. This is explicit review, not automatic name cleanup. Forge replaces exactly
one original frontmatter name line and uses the matching output directory, preserving every other
entry byte and companion. The unchanged original entry is retained in `upstream-notices/source-entry.md`;
`adaptation.json` records the approval and emitted digest. Accepted file `digest`/`size` describe emitted
bytes; optional `source_digest` identifies the original entry. Absent fields are omitted, leaving older
accepted entries unchanged. Changed original entry/hash/name or withdrawn approval requires review;
verification checks the literal transformation, source notice and ownership. Four approved adaptations
currently use this path; no global rename or runtime permission is implied.
