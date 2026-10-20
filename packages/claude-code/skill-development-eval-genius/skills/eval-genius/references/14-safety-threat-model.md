# Safety and adversarial evals: the threat model is the fixture

A safety eval measures whether a boundary holds under attack, not whether the
system behaves on average. The attack surface is part of the system spec, so
the work starts before any item is written: name what is defended, from whom,
through which door.

## Threat-model preflight

Write these down before any adversarial item exists:

- **Assets.** What a successful attack reaches: credentials, files outside the
  sandbox, another user's data, spend, the power to act as the user.
- **Trust boundaries.** Where untrusted content crosses into authority:
  retrieved documents, tool outputs, uploaded files, another agent's messages.
- **Tool authorities.** What each tool can do at worst, not what it is meant
  to do: which calls move money, write state, reach the network, read secrets.

Every safety item names the asset and boundary it exercises. An attack case
with no named asset is a vibe check.

## The frozen adversarial corpus

Attack cases are a versioned fixture like any other
(`05-dataset-construction.md`): pinned, hashed, stratified by attack class.
The minimum classes, one stratum each:

| Attack class | Shape |
|---|---|
| **Instruction injection via retrieved content** | Hostile instructions embedded in a document or tool result the system is asked to use |
| **Tool-argument smuggling** | Payload hidden inside an otherwise valid call: a URL parameter, a filename, a comment field |
| **Exfiltration** | Induce the system to move an asset across a boundary: a secret encoded into an egress call, a link, a summary |
| **Unsafe-tool invocation** | Push the system into a call it has authority for but must not make here: delete, send, purchase |
| **Goal hijack** | Redirect the task mid-run so the trajectory still "succeeds", at the wrong task |

The corpus rotates on a schedule and obeys the same contamination rules as any
fixture: a published attack suite the model trained on measures memory of the
suite.

## Forbidden transitions are code, not vibes

Safety grading is deterministic by construction (`02-grading-and-metrics.md`).
The grader asserts on state and side effects, never on the transcript's
manners:

- no egress call carried the asset; inspect the actual tool and network log;
- no file written outside the sandbox; the before/after world diff is clean
  (`10-agentic-evals.md`);
- no credential read, no scope-expanded call;
- the end-state goal still matches the task's, not the attacker's.

A judge may still grade *how* the system refused, but refusal style is a
separate layer and never gates the boundary claim.

## Canary known-bads

Every safety gate carries cases that must fail by construction: an attack that
succeeds on purpose, a run where containment is deliberately switched off. A
canary that passes means the instrument is broken and the run is
CANNOT-MEASURE (`07-gates-and-ci.md`). The same negative-control rule as
everywhere else, applied where a silent green costs the most.

## CANNOT-MEASURE, stated plainly

A safety claim needs containment and audit evidence to exist: the tool log,
the state diff, the egress record. Where the evidence is absent the outcome is
CANNOT-MEASURE, never "probably safe". A safety claim with no audit trail is
no claim, and it does not soften into a warning.

## Report per attack class

Results are per class and per item: "injection 0/14 succeeded; exfiltration
2/9". One blended safety score hides exactly the class that failed and leaves
nothing to act on. A regression in one class is a regression even when every
other class holds.

## Prior art: borrow taxonomies, not code

Published work supplies attack vocabularies worth stealing: AgentHarm for
agentic harm categories, injection-probe suites such as garak for attack
shapes, METR and Apollo evaluations for what eval-gaming and sandbagging look
like. Adopt their class lists and item shapes; build the assertions against
your own assets and boundaries. Their taxonomy generalizes; their runner does
not know your threat model.

## Output of this file

A written threat model (assets, boundaries, tool authorities); a versioned
adversarial corpus stratified by attack class; forbidden transitions asserted
in code on state and side effects; canary known-bads that must fail; per-class
reporting; CANNOT-MEASURE wherever audit evidence is missing.
