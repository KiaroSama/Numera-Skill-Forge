# Decision-model judge: typed decisions on the residue

**Positioning note.** Teach a decision-model judge / typed-question judge pattern,
not a vendor dependency. Keep provider names in the concrete example below and
executable identifiers only.

An optional instrument, never a new core layer. Code checks every item first. A
typed-question judge grades only the residue code cannot settle, and enters as
**DISCOVERY_ONLY**. Native per-choice probabilities make routing explicit; they
do not establish calibration on the user's distribution.

## When to suggest it

Trigger on the *work*, not on the key: at judge-selection, whenever the grader
plan has binary or closed-label judged dimensions, surface the lane. Then branch
on whether `TYPESAFE_API_KEY` is present in the environment (check presence
without printing the key):

- **Key present** — offer to use it now. Presence permits the suggestion, not a
  hosted call; the user still opts in to sending fixture text before anything runs.
- **Key absent** — still suggest, and offer to connect: name what the lane needs
  (a hosted decision-model key set as `TYPESAFE_API_KEY`) and ask whether they
  want to wire it up. Never fetch, guess, or hard-code a key; never call the API
  until a key is set and the user has opted in. If they decline, today's path is
  untouched — no imports, no new dependencies, no repeat nagging that run.

Either way the suggestion is a question, never an automatic switch, and the
data-leaves-the-machine caveat is stated before any hosted call.

Use this suggestion (key present):

> Code still checks every item first. For the typed decisions left over, consider
> the cheapest calibrated instrument for this job: a decision-model judge with
> native per-choice probabilities for confidence routing. Treat cheapest and
> calibrated as claims to earn on your labels and measured cost. It is hosted,
> so fixture text leaves the machine if you opt in. It starts DISCOVERY_ONLY and
> must clear your kappa-0.8 floor before any number bears a claim. Report the
> disagreement that hurts, not just the money saved.

Use this variant when no key is set:

> These N dimensions are typed yes/no or closed-label decisions — the sweet spot
> for a hosted decision-model judge that returns probabilities to calibrate on your labels.
> You don't have one connected. Want to wire one up (set `TYPESAFE_API_KEY`)? It
> stays optional and DISCOVERY_ONLY until it clears your kappa-0.8 floor on your
> own labels, and because it's hosted, fixture text would leave your machine.
> Say the word and I'll walk you through connecting it; otherwise we keep the
> current path.

## Three insertion lanes, no structural change

1. **Binary rubric dimensions.** Decompose the rubric per
   `02-grading-and-metrics.md` and calibrate per `03-judge-calibration.md`.
   Use a typed yes/no question for a dimension code cannot grade. Define what
   YES means before running; it can mean a defect exists, not that an item passes.
2. **Closed failure-mode classification.** After the human-first open coding in
   `12-error-analysis.md`, map new traces to the frozen label set. Preserve
   OTHER / NEW_FAILURE and route it to human review. The judge applies a
   taxonomy; it never establishes its completeness or authors the seed.
3. **The judge cascade.** Code checks 100% of items; the decision-model judge
   sees only the unresolved residue. Store native per-choice probabilities per
   item and apply thresholds in code. Uncertain items go to a reasoning-model
   or human layer. This upgrades the existing "two judges disagree → route"
   rule with an additional confidence signal; it does not replace disagreement
   handling. A disagreement still escalates even if a probability is high.

Keep deterministic and judged denominators separate. Never overwrite a code
failure with a judge pass or send every item to the judge by default.

## Hard excludes

- **Safety / red-team lanes (`14-safety-threat-model.md`).** Excluded: this
  instrument does not treat input as hostile. Confidence cannot certify attack
  resistance.
- **Anything needing a rationale.** Excluded: this instrument cannot explain its
  decision. The reason-before-score protocol in `03-judge-calibration.md` stays
  with reasoning judges; this narrow lane is eligible only when no rationale is
  required, with the remaining calibration rules unchanged.
- **Exact-score dimensions.** Excluded: fractional outputs do not establish
  numeric accuracy. Bands or ordinal labels need their own calibration.
- **Open-weight clones as the judge.** Excluded from this lane; the hosted
  example's behavior and calibration do not transfer to another model.

The [documented jaggedness](https://docs.typesafe.ai/model-jaggedness/jev-1.13)
names nine failure modes supporting these boundaries:

1. Literal interpretation that misses intended conditions.
2. Math, counting, and numeric representations.
3. Date and time comparison.
4. Indirection and multi-hop reasoning.
5. Irrelevant long state, causing context rot.
6. Adversarial content, including prompt injection.
7. Contradictory instructions and criteria.
8. Structural invariants that fail across separately phrased questions or types.
9. Generation, including attempts to synthesize text by chaining choices.

Keep arithmetic, counting, dates, and consistency checks in code. Filter state,
make questions direct, and use a reasoning/generative tier where needed. The
adversarial failure mode specifically backs the safety/red-team exclusion;
high confidence does not remove it.

## Confidence routing is a deployment decision

For a `noul` yes/no question, the answer value is just P(YES), with no separate
confidence field. Preserve it, derive P(NO)=1-P(YES), and record polarity.

- `noul > HIGH`: auto-accept the proposition.
- `noul < LOW`: auto-reject the proposition.
- Otherwise, including equality at either boundary: escalate to a reasoning
  model or human. Missing, invalid, or incompatible probabilities never accept.

**ILLUSTRATIVE defaults: LOW=0.3, HIGH=0.7.** These are not production
defaults. If YES means "real defect," accepting the proposition flags a defect;
it does not pass the item. For multi-choice questions, pre-register a separate
routing policy over the full probability vector; do not apply P(YES) thresholds
to the winning class's confidence.

Illustrative example, **hypothetical, not measured evidence**: suppose a
"real defect?" question scores clear defects around 0.63–0.83, clear noise around
0.06–0.22, and an ambiguous item at 0.62. That pattern suggests a routing
experiment, not a floor passed: raw separation is not kappa, and the ambiguous
band overlaps your thresholds. Treat any such numbers as a starting hypothesis to
test on your own labels.

**Calibrate LOW and HIGH per deployment from the user's OWN labels. Never
hard-code an illustrative threshold or vendor example such as >0.95.** Tune on a
calibration split, freeze the policy, then evaluate untouched labels. Report
false accepts, false rejects, escalation rate, per-class errors, and coverage
alongside agreement. DISCOVERY_ONLY auto-routes are diagnostic suggestions,
never production PASS/FAIL or headline claims.

`scripts/jev_confidence_route.py` is the opt-in confidence-routing example.
Persist item ID, code disposition, question and label-set version, model version,
prompt hash, per-choice probabilities, LOW/HIGH, route, final label and its
source, and metric role so a decision can be reconstructed.

### Choice/score routing

**Thresholds DO NOT transfer from the yes/no router. Calibrate each question
type separately**, including its label set and wording. Even a yes/no choice
can disagree with a yes/no probability for the same proposition; independently
asked negations need not be complements.

There are only three primitives. Alongside `noul`, `choice` returns a chosen
option, `confidence`, and `probabilities{option:p}` for at most 255 options.
`score` returns a fractional, probability-weighted score, `confidence`, a
`legend`, and `probabilities{level:p}` for 2–10 levels. Preserve the vector;
the fractional score is neither an exact measurement nor its modal level.
See the [API shapes](https://docs.typesafe.ai/api).

Record top-1 probability `peak`, the top1-minus-top2 margin, and the separate
confidence scalar `(count * peak - 1) / (count - 1)`, where count is the number
of options or levels. Top-1 probability and this normalized confidence are
different signals; neither is automatically a calibrated correctness rate.

`scripts/jev_choice_route.py` routes choice/score JSONL with
`--by confidence|margin`, `--high`, and `--low`. Above HIGH, accept the proposed
decision; below LOW, reject that proposal without inventing an alternative
label. The inclusive LOW-to-HIGH band abstains and escalates, so **equality at
either boundary escalates**. A low-confidence rejection does not establish that
the underlying item failed. Missing or invalid data cannot auto-accept. Preserve
OTHER / NEW_FAILURE and disagreement overrides for human review. In this lane,
all automatic routes remain DISCOVERY_ONLY until the agreement gate is earned.

### Calibration: fit on your own labels

**Calibrate before thresholding, then measure agreement.** The vendor publishes
no expected calibration error (ECE) number; third-party calibration figures are
unverified. There is no vendor-derived correction constant to copy into code.

`scripts/jev_calibrate.py` fits temperature scaling from the user's `{raw,label}`
JSONL and refuses fewer than 50 items. Fit each question type and task on its
own labeled calibration split; a fitted temperature is an estimate, not proof
of calibration. Keep raw outputs, the fitted transform, split provenance, and
model/question versions. Apply the frozen transform consistently before tuning
and freezing the routing thresholds. Do not fit to the untouched agreement set.
For choice/score, transform the vector and recompute derived confidence/margin;
never combine transformed probabilities with stale native confidence.

Check reliability on held-out labels, with per-class errors and coverage, then
run `scripts/judge_agreement.py` for the kappa gate below. Temperature scaling
cannot repair missing classes or a bad rubric, and improving calibration does
not by itself improve categorical agreement. The 50-item fitting minimum does
not replace the separate held-out agreement minimum.

### Batching: many questions per item, one call

Batch independent questions about one item's shared state into one call.
Questions are isolated: they cannot see one another's answers. A genuinely
dependent question needs a later call with its required evidence. This is not
permission to concatenate unrelated items or skip deterministic checks.

The [parallel-question example](https://docs.typesafe.ai/primitives#ask-speculative-questions)
reports roughly **12× cheaper and 10× faster** for batching, a vendor example,
not a promised multiplier for your workload. Measure your own usage and latency.
The request budget is **64,000 tokens for state plus all questions**. Current
[model limits](https://docs.typesafe.ai/models) also cap state plus the longest
question at 32,000 tokens; both constraints must hold before a hosted call.

Use the extended `scripts/jev_cascade_cost.py` with `--questions-per-item`,
`--state-tokens`, and `--question-tokens` to compare batching against separate
calls. For Q equal-sized questions, approximate input tokens per judged item
as `state_tokens + Q * question_tokens` batched, versus
`Q * (state_tokens + question_tokens)` unbatched. Include instructions and
criteria in question tokens and allow for request overhead. Isolation between
questions does not cure context rot inside a large shared state.

## Cascade cost: pay only for the residue

Let r be the unresolved fraction after deterministic checks on 100% of items.
With average input tokens per judged item and price in dollars per billion input
tokens, effective judge input cost per original item is approximately:

`r × (tokens_per_item / 1e9) × price_per_btok`

The human/reasoning **escalation tier is a real cost**, not a free fallback.
Let e be the fraction of judged residue escalated and c the dollar cost per
escalated item, including paid human time or the reasoning call:

`cost_per_original_item = r * (tokens_per_item / 1e9 * price_per_btok + e * c)`

Multiply by total item count for run cost. `scripts/jev_cascade_cost.py` adds
`--escalation-rate` (fraction of judged residue, not all original items) and
`--escalation-cost-per-item` (dollars). For several questions on one item, count
the item once if any question triggers item-level review; do not confuse that
rate with the per-question rate. An unspecified or zero escalation budget is
an assumption to disclose, never evidence that human review is free.

The estimate still excludes code runtime, retries, and other unmodeled charges.
Include them in measured total cascade cost before claiming savings. Compare
against the existing code-first judge plan on the same fixture, not an invented
judge-everything baseline.

## Earn the floor before shipping a claim

Copy `templates/jev-lane-preregistration.md` before the prototype. Start with
50–100 real labeled items, including hard cases and negative space. This is a
prototype budget, not a replacement for the larger calibration set and usable
agreement interval required by `03-judge-calibration.md`. Have humans agree on
the guideline first; keep threshold tuning separate from held-out verification.

Reuse `scripts/judge_agreement.py` as-is to measure chance-corrected agreement
against the user's own labels. It requires at least 50 shared IDs per comparison
and 80% coverage of each label file; a 50-item prototype split into tuning and
holdout cannot clear that requirement. Expand the held-out set, never relax the
guard. Use distinct `id,label` files and pass the complete closed label set via
`--labels` with an explicit `--positive` and `--floor 0.8` (or higher).
Check both the judged residue and the final
cascade on the held-out set, auditing code-cleared items too; report routed
subsets so easy items or human escalations cannot hide a weak auto-graded lane.
Human escalation labels must be independently verified, not scored against
themselves. Expand the sample if class support or uncertainty is inadequate.

The lane earns claim-bearing status only at **kappa >= 0.8** on those labels
(or the higher pre-registered floor where required). Ship the claim-bearing lane
only if the cascade holds that agreement floor **while cutting measured judge
cost**. Otherwise it stays a DISCOVERY_ONLY diagnostic suggestion. A low price
cannot buy its way past the floor. Recalibrate on model, prompt, taxonomy,
threshold, or distribution changes. No changes to `judge_agreement.py`,
`paired_bootstrap.py`, or binary `check_gate.py` are needed.

## Concrete example (opt-in, hosted only)

**Concrete example:** TypeSafe AI's Jev (System One), model `jev-latest`, via
`POST https://api.typesafe.ai/v1/systemone` with Bearer `$TYPESAFE_API_KEY`;
Choice / Score return a decision, probability vector, and separate confidence;
Noul returns P(YES) directly ([API quickstart](https://docs.typesafe.ai/introduction/quickstart)).
Pin this example's endpoint and request model to `/v1/systemone` and
`jev-latest`. The gateway variant `/v1/decisions` with `typesafe/jev` exists
but is not used here. The alias can move: record the resolved response model
and invalidate calibration when it changes.
The [published input price](https://docs.typesafe.ai/models) is $42 per billion
input tokens, output free, checked on 2026-09-22; verify before a hosted run.
Score support does not remove the exact-score exclusion. This is an example of
the pattern, not a core dependency or an independently verified price comparison.

## Output of this file

An explicitly opted-in, code-first judged-lane plan: typed dimensions and their
excludes, hosted-data consent, per-item probabilities with a frozen routing
policy, measured cascade cost, and a DISCOVERY_ONLY role until the user's own
held-out labels earn the agreement floor.
