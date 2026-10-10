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
TLS verification stays enabled. Unsafe inherited Git/HTTP overrides return `provider-environment`.
Cached repository configuration is checked before use; extra includes, alternate object stores or
unreviewed settings return `cache-policy` without changing accepted snapshots. The existing process
owner supplies deadlines, cancellation and pipe cleanup; upstream blobs/hooks are never executed.

`credential_reference` is only a public key name matching `[A-Za-z][A-Za-z0-9_-]{0,63}`; no token,
URL, file path or secret-store instruction belongs in the registry. Registration does not read its
value. Online acquisition with a reference returns `credential-provider-unavailable`: an approved
store/environment boundary capable of isolating secrets from argv, logs and inherited process
settings is not integrated. No token is written to a temporary file and no user credential store is
read by the new acquisition provider. Existing cached snapshots remain usable offline.

Reviewed SSH registration uses `ssh://git@forge.example.invalid/team/skills.git` and the same exact
policy with `transport: "ssh"`. Online acquisition returns `ssh-provider-unavailable`; it does not
invoke SSH or read user SSH configuration/keys. A future provider must enforce approved identity
selection, pinned host keys, strict host-key checking and noninteractive operation before support
can be claimed. SCP shorthand and fallback to ambient SSH credentials are not supported.

The existing GitHub `latest-stable-release` metadata lookup remains a separate `gh api` helper,
with the CLI's ambient authentication/configuration behavior; it does not resolve source
`credential_reference` or authenticate private clones. Custom-host release APIs are unsupported.
Actual custom HTTPS/private/SSH acquisition has not been integration-tested without an approved
endpoint and, for authenticated transports, the missing secure runtime boundary.

Package fields: `id`, `name`, `source`, `components`, `dependencies`, `targets`, `optional_exclusions`.
Unknown host APIs require a capability/adapter change and regression tests; adding more content to an
existing valid profile does not. Unknown/missing licenses block redistribution pending review.

## Exact skill collections

`registry/collections.json` records explicitly reviewed skill roots, immutable initial revision and
license path/digest. Original grants omitted by the aggregator can be supplied as `notices` with
tracked original legal-text path, exact digest and immutable source URL; adaptation attribution stays
outside the unchanged skill payload. MIT, Apache-2.0 and CC-BY-4.0 remain distinct grants. Exact
reviewed public environment templates use `reviewed_templates`; real secrets remain forbidden.
See [collection schema](../schemas/collections.schema.json). Each skill gets one plugin. `collection sync --apply` retains complete original
files and executable modes; `collection verify` checks accepted payloads and native catalog without
execution. Adding a reviewed collection or skill is registration data, not an engine edit.
See [licensing](source-licenses.md) and [daily/local updates](upstream-sync.md).
