# Source and Package Registration

New content for an existing supported profile is data, not engine code. The active initial registry
starts empty; the owner-approved davila7 collection is registered separately in `registry/collections.json`. Examples never become approved upstreams automatically.

1. Register source with a unique lowercase ASCII ID, credential-free approved HTTPS URL or explicit
   `local:` absolute trusted Git repository path, selected roots and targets.
2. Discover metadata and companion files without running source scripts.
3. Review license, component inventory and candidate content digest.
4. Explicitly approve the exact digest; executable content requires separate trust approval.
5. Register an independently installable package, then build its target with accepted locked inputs.

See [example package record](../examples/package.json). Components omitted/empty selects discovered
components; required unsupported conversions fail instead of dropping material. Dependencies must
reference existing packages and cannot form cycles.

Registry schema v1: `schema_version`, ordered `sources`, `packages`. Source fields: `id`, `repository`,
`enabled`, `reference` (strategy/default-branch, branch/commit/tag with value), `roots`, `excludes`,
`targets`, `update`, `credential_reference`, preserved metadata. Sources default disabled/review.

Package fields: `id`, `name`, `source`, `components`, `dependencies`, `targets`, `optional_exclusions`.
Unknown host APIs require a capability/adapter change and regression tests; adding more content to an
existing valid profile does not. Unknown/missing licenses block redistribution pending review.

## Exact skill collections

`registry/collections.json` records explicitly reviewed skill roots, immutable initial revision and
license path/digest. Each skill gets one plugin. `collection sync --apply` retains complete original
files and executable modes; `collection verify` checks accepted payloads and native catalog without
execution. Adding a reviewed collection or skill is registration data, not an engine edit.
See [licensing](source-licenses.md) and [daily/local updates](upstream-sync.md).
