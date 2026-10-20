# Grading and metrics: deterministic first, judged last

The first fork in any eval is who grades it. This choice drives cost, reproducibility,
and how much the number can be trusted.

## The fork

| | **Deterministic** (code-graded) | **Non-deterministic** (model- or human-graded) |
|---|---|---|
| Grader | Exact match, normalized match, regex, schema/type check, test suite, numeric threshold, set overlap | LLM-as-judge, human rater, pairwise preference |
| Use when | A checkable right answer exists | Quality is inherently a judgment: tone, helpfulness, faithfulness |
| Cost | Free, instant | Slow, paid, noisy across runs |
| Trust | High, if the check itself is correct | Only as good as the judge's calibration |

**Governing rule:** push every check that can be deterministic down to code. Reserve
judgment for the edge no assertion captures. Then report the layers separately: a
deterministic pass rate and a judged score never share a denominator or a headline.

## Making more things deterministic

Before accepting "this needs a judge," try these moves in order:

1. **Normalize then match.** Lowercase, strip whitespace and punctuation, canonicalize
   numbers and dates. Most "fuzzy" answers become exact.
2. **Extract then match.** Have the system emit a structured field (JSON, a final line,
   a tagged span) and grade the field, not the prose.
3. **Assert on state, not text.** For agents and code, grade the resulting filesystem,
   database, or test suite, not the transcript.
4. **Decompose the rubric.** "Is this a good summary" becomes "contains fact A," "contains
   fact B," "contains no fact outside the source." The first two are set-membership
   checks. Only the last may need a judge, and often a fact-list diff does it.
5. **Use a reference set with tolerance.** Numeric answers within an interval; lists
   graded by Jaccard overlap above a threshold.

Only what survives all five becomes the judged layer.

## The judge cascade: filter with code, judge only the suspects

Deterministic-first is a selection rule; the cascade is a runtime architecture that keeps
judge spend near zero at volume. Run the cheap code check on **every** item, and send only
the items that trip it to the judge. A tree-shape example: count children per node in code,
and pass only the over-full nodes to a judge for the semantic "should these be grouped"
call. The judge never sees the items code already cleared, so cost scales with the failure
rate, not the item count. It needs no special harness: the scorer routes, the judge grades
the residue. This is how a harness-agnostic eval stays affordable on a large fixture.

For an opt-in typed-question judge on binary/closed-label residue, see `15-decision-model-judge.md` for confidence thresholds, cascade cost, and the earn-it agreement floor.

## Partial credit vs binary verdicts

A multi-part task can be partly right in a way a single pass/fail throws away: an agent
that identifies the problem and verifies the customer but botches the refund is meaningfully
better than one that fails at the first step. Two rules, used in different places:

- **For measuring capability**, award partial credit: decompose the task into components,
  score the fraction done, and carry that per-item fraction in the record's `score` field
  (which `paired_bootstrap.py` already reads). This is how you see a system getting better
  before it gets all the way there.
- **For a gate**, stay binary. `check_gate.py` verdicts are `pass`/`fail`/`error` by design:
  a merge decision is yes or no, and a partial score invites arguing the bar after the fact.

The split is deliberate. Partial credit measures progress; binary verdicts guard the merge.
Say which one a given number is.

## Metric catalog by task family

Pick from the family that matches the promise. Each entry names the failure mode of
the metric so it can be caught.

Every metric holds exactly one of two roles, written in the manifest next to it:

- **DISCOVERY_ONLY.** The metric may rank candidates, select items for human
  review, flag outliers, or stratify a sample. It is barred from PASS/FAIL and
  from every headline claim, because it was never validated against a named
  failure mode. Generic quality scores, novelty flags, and heuristic ranks
  live here.
- **Claim-bearing.** The metric maps to a validated failure mode (one the
  error analysis or the threat model named), carries a stated direction,
  unit, and bar, and may appear in a verdict.

A discovery metric that starts deciding is the discovery-metric-as-claim
anti-pattern. When a discovery heuristic selects the slice that gets measured,
record the sampling policy and each item's inclusion probability; otherwise a
"we measured the flagged items" number reads as prevalence, which it is not.

### Retrieval / search / memory

| Metric | Measures | Watch for |
|---|---|---|
| recall@k | Is the relevant item in the top k | Ignores rank inside k; pick k from the real UI |
| MRR (mean reciprocal rank) | How high the first relevant item lands | Dominated by first hit; blind to second relevant item |
| nDCG@k | Graded relevance with position discount | Needs graded labels; cheap labels make it meaningless |
| precision@k | How much of the top k is relevant | Punishes systems that surface useful near-misses |

Retrieval gates work best on a per-query diff, not the aggregate: a mean can hold
steady while ten queries collapse and ten others improve.

### Classification / routing / triggering

| Metric | Measures | Watch for |
|---|---|---|
| Precision / recall / F1 | Trade-off between false alarms and misses | Report both, never F1 alone; class imbalance hides in accuracy |
| False-positive rate on benign input | The negative space | Requires an explicit benign set, which teams forget to build |
| Confusion matrix | Which classes get confused | The only view that explains *why* F1 moved |

### Generation / summarization / rewriting

| Metric | Measures | Watch for |
|---|---|---|
| Key-fact coverage | Fraction of pre-listed facts present | Needs a fact list per item, built before generation |
| Fabrication count | Facts present that are not in the source | Best graded by fact-list diff, then a judge only on disputed items |
| Constraint compliance | Length, format, forbidden terms, required sections | Fully deterministic; always run first |
| Judged quality | Fluency, tone, helpfulness | The judged layer; calibrate per `03-judge-calibration.md` |

Lexical overlap scores (ROUGE, BLEU) are diagnostics, not outcomes. They reward
copying and punish good paraphrase.

### Extraction / structured output

| Metric | Measures | Watch for |
|---|---|---|
| Schema validity rate | Output parses and conforms | Necessary, not sufficient |
| Field-level exact match | Per-field correctness | Report per field; a single aggregate hides the one field that always fails |
| Slot F1 | Precision/recall over extracted entities | Define matching rule (exact vs normalized) up front |

### Code

| Metric | Measures | Watch for |
|---|---|---|
| Test pass rate | Hidden tests pass | Tests must be hidden from the system; leaked tests inflate |
| pass@k | Any of k samples passes | Reports capability, not reliability |
| pass^k | All k samples pass | Reports reliability; the number a user actually feels |
| Diff scope | Files touched outside the task | Catches collateral damage that tests miss |

### Agentic / multi-turn

See `10-agentic-evals.md`. Outcome (final state) and trajectory (how it got there) are
graded separately.

### Customer feedback (production signal, feeds error analysis)

One more grader source alongside code assertions and judges, and the only one that
measures what users actually think rather than what a rubric predicts. It lives at the
monitor stage and its findings feed `12-error-analysis.md` as new error categories.

| Signal | Measures | Watch for |
|---|---|---|
| Explicit rating (thumbs, stars) | Stated satisfaction | Low information: a thumbs-down rarely says *what* was wrong, so you infer; sparse and biased toward extremes |
| Regeneration | User asked for the answer again | The clearest implicit "that was wrong"; count the rate per feature |
| Edit-then-retry | User changed the prompt before rerunning | The first output missed; the edit shows what was missing |
| Follow-up question | User had to ask again to get there | The first answer was incomplete |
| Edit of the output | User fixed the answer by hand | The edit is a free labeled diff of exactly what was wrong; the richest signal there is |

Implicit behavioral signals beat explicit ratings: they are dense, unprompted, and each
one points at a specific failure. Mine them for error categories, then measure those
categories with the deterministic and judged graders above.

### Cost and latency (always, alongside any of the above)

| Metric | Measures | Watch for |
|---|---|---|
| Tokens in / out per item | Spend | Judge tokens count too; report them separately |
| Cost per pass | Money spent per successful item | The metric that makes "more retries" honest |
| Latency p50 / p95 | Typical and tail response time | Means hide tails; tails are what users feel |

A quality gain that doubles cost or tail latency is a trade, not a win, and the report
says so.

## When the subject games the grader

`05-dataset-construction.md` covers the author-side version: leaked tests,
loopholes in the task spec. The subject-side version is worse: a capable
system under test probes the verifier itself and passes by producing what the
grader keys on rather than what the task asks. Four defenses:

- **Hidden holdout.** A slice of items and checks the subject never sees, in
  any run. A grader the subject can read is a grader it can fit.
- **Adversarial verifier tests.** Feed the verifier deliberately gaming-shaped
  outputs: the right tokens with no work behind them, a refusal-shaped answer
  with the exfiltration still inside, a hidden-test file read mid-trajectory.
  The verifier must catch each one; a verifier that cannot tell the artifact
  from the work is measuring artifacts.
- **Process-vs-outcome cross-check.** End state correct but the trajectory
  shows the shortcut (the leaked file read, the test copied, the assertion
  edited): fail the item and flag the check that allowed it. Outcome and
  trajectory stay separate grades (`10-agentic-evals.md`); this is the one
  place they are read together.
- **Mutation tests.** Synthesize plausible cheating trajectories, by mutating
  known-good runs toward the exploit, and confirm the gate still goes red.

The rule that binds them: **an exploitable verifier invalidates the run.**
When the subject passes through the instrument rather than the task, every
number above that instrument is CANNOT-MEASURE, not a pass with an asterisk.
Fix the verifier, re-run, then claim.

## Choosing k, thresholds, and tolerances

Pull them from the product surface, not from habit. If the interface shows five
results, recall@5 is the metric. If a user waits at most two seconds, p95 under two
seconds is the bar. Thresholds invented in the abstract get gamed by the abstract.

## Output of this file

A grader plan: which checks are deterministic (and their normalization rule), which
items go to the judged layer and why, the metric per outcome with direction, unit,
and role (DISCOVERY_ONLY or claim-bearing), a verifier hardened against subject
gaming, and cost/latency recorded next to quality.
