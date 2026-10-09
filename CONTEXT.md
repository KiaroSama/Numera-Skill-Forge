# Extension Distribution

Numera Skill Forge curates source-aware extensions for coding harnesses, without conflating
individual capabilities with packages or catalogs.

## Language

**Source**: A registered repository or explicitly selected local input and its selection policy.
_Avoid_: Package, plugin

**Snapshot**: Immutable accepted content from a source, with origin and license evidence.
_Avoid_: Observed head, latest download

**Component**: One typed extension unit, such as a skill, hook, agent or runtime mod.
_Avoid_: Plugin when referring to a single unit

**Package**: An independently installable versioned group of components and dependencies.
_Avoid_: Catalog, source

**Plugin**: A package in a harness's documented native or portable plugin format.
_Avoid_: Skill-only directory when the host expects runtime registration

**Catalog**: A listing of packages, not an executable extension.
_Avoid_: Plugin, marketplace payload

**Capability**: Documented support for a component on a harness surface/version/platform.
_Avoid_: Universal compatibility

**Observed revision**: The newest upstream revision inspected, whether accepted or not.
_Avoid_: Accepted snapshot

**Accepted revision**: The reviewed revision used for distributed content.
_Avoid_: Latest head

**Trust approval**: Consent bound to package identity and accepted content/capability digest.
_Avoid_: Blanket owner trust

**Receipt**: Ownership and resulting-content evidence for an installation operation.
_Avoid_: Approval, snapshot
