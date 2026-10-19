# Missing skill imports

The import adds **72 independently selectable packages**, bringing the accepted inventory
to **1110 packages**. The accepted lock records every payload/notice hash and mode. All **1038 previous accepted package
records** remain unchanged. Importing instructions never executes scripts, installs hooks or
activates external services.

## What was added

- **37 name-only adaptations** from the pinned Templates source. Only the approved technical name
  and matching directory change. Each exact original entry remains in
  `upstream-notices/source-entry.md`, with both identities and the change in `adaptation.json`.
  Bodies, companions, copyright notices and modes are unchanged; source changes require review.
- **Nine complete original-source packages** resolve incomplete intermediary copies. They use the
  actual original entry and full root, not a claim that the incomplete mirror was identical.

| Missing intermediary root | Complete source | Reviewed revision |
| --- | --- | --- |
| `web-development/zustand-store-ts` | [Microsoft skills](https://github.com/microsoft/skills/tree/2f9d135614ca76198a2fb715052c7c23b2eead5b/.github/skills/zustand-store-ts) | `2f9d135` |
| `workflow-automation/n8n-workflow-patterns` | [n8n skills](https://github.com/czlonkowski/n8n-skills/tree/d9c287202999481777868c4ce7441ced847350b3/skills/n8n-workflow-patterns) | `d9c2872` |
| Four `productivity/notion-*` skills | [OpenAI curated skills](https://github.com/openai/skills/tree/dadec22b4a3501c2b7263a644aee44ff26228ee0/skills/.curated) | `dadec22` |
| `development/gh-address-comments`, `development/gh-fix-ci` | [OpenAI curated skills](https://github.com/openai/skills/tree/dadec22b4a3501c2b7263a644aee44ff26228ee0/skills/.curated) | `dadec22` |
| `development/skill-installer` | [OpenAI system skills](https://github.com/openai/skills/tree/dadec22b4a3501c2b7263a644aee44ff26228ee0/skills/.system/skill-installer) | `dadec22` |

The Notion packages are `notion-knowledge-capture`, `notion-meeting-intelligence`,
`notion-research-documentation` and `notion-spec-to-implementation`. They retain original Notion Labs
MIT grants. OpenAI's three tool skills retain scoped Apache-2.0 licenses. Microsoft and n8n retain
original MIT grants. Each source has independent collection/update identity.

## Original engineering guides

A further22 packages use the licensed [wshobson originals](https://github.com/wshobson/agents/tree/ade0c7a211d04fa1354d10359737cace5c6fc5b0):
three complete native skills (`llm-evaluation`, `database-migration`, `react-state-management`) and
19 original agent guides. Each guide adapter keeps the original Markdown file unchanged beside a
short, explicitly Forge-authored SKILL.md. It references the complete guide and does not launch an
agent, inherit its model/tools or claim it was an upstream-native skill. Exact path/hash/version1
approval is required; source drift or withdrawal refuses. Sibling agents are not bundled.
The incomplete intermediary copies are not presented as exact originals.

Three more complete native originals are restored from author revision
[`0874a27`](https://github.com/wshobson/agents/tree/0874a27b17a22e2b7c2e2bcd725768e0803cad06):
`grafana-dashboards`, `prometheus-configuration` and `github-actions-templates`. The author removed
phantom resource requirements; Prometheus now includes its required `references/details.md`.
All four original skill files and three copies of the unchanged MIT license are preserved. No
missing historical asset was fabricated, no source instruction was locally edited, and all1107
previous accepted records remain unchanged. The collection now contains25 packages:6native skills
and19guide adapters, each retaining its own accepted revision.

The19 guide names are `ml-engineer`, `database-optimizer`, `c-pro`, `cloud-architect`, `csharp-pro`,
`django-pro`, `elixir-pro`, `fastapi-pro`, `flutter-expert`, `graphql-architect`, `haskell-pro`,
`incident-responder`, `java-pro`, `monorepo-architect`, `php-pro`, `ruby-pro`, `scala-pro`,
`tdd-orchestrator` and `debugger`. Each retains the original Seth Hobson MIT license.

## Exact CRLF original

The complete27-file `ui-ux-pro-max` root is now accepted unchanged from the pinned Templates
source, with original Next Level Builder MIT and applicable intermediary notices. Forge reads
LF/CRLF frontmatter without normalizing entry bytes, companions or modes. Its exact original
native-variant provenance remains explicit; imported Python scripts were not executed.

## Install and update

Package IDs retain the intermediary category/name convention, for example:

```bash
claude plugin install skill-pocketbase-pb-api-rules@numera-skill-forge
```

```bash
claude plugin install skill-productivity-notion-knowledge-capture@numera-skill-forge
```

Use the existing [host setup](host-setup.md) and [upstream update rules](upstream-sync.md).
Native descriptors do not register hooks, MCP servers or executable runtime integration.
Notion/GitHub credentials and tools remain separate prerequisites. Security instructions are for
authorized use only; inclusion is not permission to test a target.

## What remains outside

Of the previously identified **153 missing canonical roots**, **72 now have packages** and **81
remain unbundled**:77 Templates roots plus four scientific Office roots. The [current per-root table](canonical-root-outcomes.md) accounts for every one of the153 roots.
The historical [hold table](remaining-skill-disposition.md#held-roots-and-exact-next-condition)
is superseded for these72 resolved rows.

- Scientific `docx`, `pdf`, `pptx` and `xlsx` contain specific proprietary terms restricting copying
  and redistribution. A permissive fork-root label does not replace those scoped terms.
- All25 intermediary playbook cases are resolved through complete author originals/guides, including
  the author's repaired Grafana, Prometheus and GitHub Actions roots. Playwright Java and NoWait still
  reference unavailable paths in their selected layouts; neither was silently relocated.
- Other original-rights, name-and-origin, mixed-notice and modification-compliance holds need their
  specific evidence or reviewed repair. Technical approval is not a license grant. Native DoorDash
  enforcement needs components beyond a standalone skill.
- The CRLF-only hold is resolved without rewriting source line endings. Every remaining held row
  has a concrete next-admission requirement in the current per-root table.

Generated Antigravity copies are delivery duplicates, not extra canonical skills. Existing shared
scientific versions are not replaced by fork variants. Packaging and installation evidence do not
certify model-followed behavior, upstream scripts or external APIs.
