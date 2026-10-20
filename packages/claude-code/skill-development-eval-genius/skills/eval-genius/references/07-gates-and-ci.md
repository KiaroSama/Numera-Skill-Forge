# Gates and CI: making the eval block the merge

A gate is an eval with the power to say no. That power is only worth having if the
gate is trusted, and a gate is trusted only when it is fast enough to run, honest about
when it could not run, and impossible to argue with after the fact.

## Tiers

| Tier | Runs on | Budget | Content | On failure |
|---|---|---|---|---|
| **Smoke** | Every commit | Under a minute, no paid calls | Deterministic subset, negative control, liveness | Block |
| **Gate** | Every pull request | Minutes, small paid budget | Full deterministic layer on the held-out set; judged layer on a fixed sample | Block |
| **Nightly** | Scheduled | Up to an hour, moderate budget | Full judged layer, repeated trials, fresh-build run, cost and latency | Alert, block release |
| **Release** | Before a version ships | Whatever it costs | Everything, plus any adopted public benchmark | Block ship |

Anything that cannot fit a tier's budget moves up a tier; nothing is dropped silently.
The tier a check runs in is written next to the check.

## What a gate compares

Per-item, against a committed baseline, on the same fixture fingerprint. The rule is
written in the pre-registration and looks like one of:

- **No regressions:** no item that passed in baseline fails now, tolerance zero. Strict
  and right for deterministic layers.
- **Aggregate with tolerance:** metric not below baseline minus a stated tolerance, and
  the per-item diff attached so a masked collapse is visible.
- **Interval rule:** the paired confidence interval on the delta excludes a regression
  larger than the stated tolerance (`08-statistics.md`).

A gate rule with no per-item diff is a gate that can be fooled by offsetting changes.

## Baseline lifecycle

- **Lives in the repo**, versioned, as per-item records plus the manifest, next to the
  fixture hash it was produced on.
- **Promotion** is a deliberate commit with a written reason: "baseline updated after
  change X; delta +0.031 recall@5, 0 regressions, manifest attached." Nobody promotes a
  baseline in the same change that needed it to pass.
- **Re-baseline** whenever the fixture, the judge, or the harness version changes,
  because the old numbers are no longer comparable. Say so in the commit.
- **Never edit** a baseline by hand.

## Suite lifecycle: capability, regression, graduation

Two kinds of suite sit behind the same gate, at opposite ends of the pass-rate
axis:

- A **capability suite** starts low and climbs. It measures a hill the system is
  still climbing; eval-driven development is a bet on where the system is going,
  and a low starting number is the shape of the bet, not a defect in the suite.
- A **regression suite** sits near 100%. It guards what already works, and every
  flip is a regression to explain, not a hill to climb.

Items move between them in one direction. A capability item that saturates (passes
on every run, across real changes, until nobody expects it to fail again)
**graduates** into the regression suite: its job flips from "show progress" to
"say no." A capability suite reading 100% across the board is finished or stale;
a regression suite sitting at 80% is either failing or misfiled.

A saturated error-detector is a different animal. An eval built to count one
failure class that now reads 100% is measuring nothing: the error left the data
(`12-error-analysis.md`). That is a signal to retire or refresh it, in writing
(`09-reporting.md`), and spend the attention on a live error class.

"Saturation" names three different events in this skill; keep them apart:

| Where | What saturated | Response |
|---|---|---|
| `04-search-vs-build.md` | A public benchmark under consideration | Adopt it as a floor check, not a headline |
| `09-reporting.md` | A benchmark you run, ceiling reached or promise expired | Retire it, in writing |
| This file | One item inside a capability suite | Graduate it into the regression suite |

## Flake policy

A flaky gate is a broken gate. Retries hide regressions and train people to ignore red.

1. Measure run-to-run variance on the same commit first (`08-statistics.md`). If the
   variance exceeds the tolerance, the gate is not yet a gate; move the noisy items to
   nightly with repeated trials, or widen the tolerance with a written reason.
2. Allow at most one automatic retry, only for CANNOT-MEASURE outcomes (infrastructure
   failures), never for FAIL.
3. Quarantine an item that flips on identical inputs, with an issue and a deadline.
   Quarantine is visible in the report, not silent.

## Verifier verification, wired in

Every gate run includes a negative control: an item known to be bad (a deliberately
wrong output, a corrupted fixture entry, a disabled feature). The gate must fail it. If
the negative control passes, the scorer is broken, the run is CANNOT-MEASURE, and
nothing else the run says is trusted. This is the cheapest check in the system and the
one that catches "the gate has been green for a month because it grades nothing."

## Triage when the gate fails

1. **Read the outcome.** CANNOT-MEASURE is an infrastructure or fixture problem; fix the
   gauge, do not touch the system.
2. **Read the per-item diff.** Which items flipped, in which direction, in which
   stratum. Ten flips in one category is a real effect; ten flips scattered at random
   on a noisy judge is variance.
3. **Reproduce one flipped item by hand** before believing the aggregate.
4. **Bisect** on the lever if the change contains more than one.
5. **Decide: fix the system, fix the gauge, or accept with a written reason and a
   baseline promotion.** Never the fourth option, which is loosening the bar until
   green.

## Cost and latency gates

Quality gates that ignore cost let a change ship that doubles the bill. Add:

- Tokens per item and cost per pass, with a tolerance.
- p95 latency, with a tolerance.

Both are per-item records already, so they ride the same diff.

## Partial results and cost ceilings

A suite that stops mid-run has not run. A cost ceiling hit at item 600 of 900, or
a rate-limit that kills the trial early, yields a **partial** result, and a
partial result is CANNOT-MEASURE: the outcome is "the instrument stopped", not a
score on the system. Two consequences:

- **Partial results never enter trend charts.** A pass rate computed on the first
  600 items was measured on a different fixture; plotting it beside complete runs
  manufactures a movement nobody caused.
- **A run that silently returns partial looks like a verdict.** If the missing
  items read as failures it looks like a regression; if the aborted tail is
  dropped it can even read green. A rate-limited or cost-capped suite that
  reads its truncated tail as failures is a silent crash in a regression's
  clothes: CANNOT-MEASURE, never FAIL, and out of the trend chart. Both are
  the green-by-crash family below: the pipeline got a number and believed it.
  The same-item-ids and error-budget checks exist to refuse exactly this
  comparison.

Treat the cost ceiling as a pre-registered budget, not an afterthought. It is
written in the pre-registration with the other bars ("abort the run past $X"),
recorded in the manifest when it trips, and reported as a stopped run. A ceiling
discovered by the invoice was never a budget.

## A third placement: inline guardrails

A gate runs before the merge; a monitor watches live traffic after the fact. A
third placement runs **inline, before the response reaches the user**: the check
sits inside the request path and can stop or fix the output, not just record it.
It is the same instrument discipline under a different cost and latency profile:

- **It must be fast.** Its latency is the user's latency. The budget is
  milliseconds or one cheap call; a judged layer or a full suite does not fit.
- **It must have a remediation path.** Recording the failure is a monitor's job.
  Inline, a failure triggers a retry of the generation or routes the output to a
  fixer call. A guardrail that can only say no either blocks good answers or gets
  bypassed.

Checks that cannot meet the latency budget or carry no remediation stay in the
gate or the monitor. The guardrail itself is measured like any eval: its
false-block rate is a cost line, and a guardrail that fires on good output is a
regression source like any other.

## Anti-patterns specific to gates

- **Green by crash.** Runner exits 0 after a setup failure; CI reads green.
- **Retry until green.** Any retry of a FAIL.
- **Baseline drift.** Baseline promoted in the same PR that needed it.
- **Gate-set tuning.** Prompts iterated against the held-out set.
- **Silent tier drop.** A slow check removed from the gate and added nowhere.

## Output of this file

A gate definition: tier, fixture hash, comparison rule, tolerance, baseline location
and promotion rule, the capability-to-regression graduation path, flake policy,
negative control, cost/latency limits with a pre-registered ceiling, refusal of
partial runs, and the exit code contract CI honors.

## CI shell contract

A gate command has three meaningful exits. CI must preserve all of them even though
the job UI ultimately shows only success or failure:

```bash
set +e
python3 path/to/check_gate.py --baseline "$BASELINE" --treatment "$TREATMENT"
code=$?
set -e
case "$code" in
  0) echo "PASS: the measured bar was met" ;;
  1) echo "FAIL: the run was valid and the bar was missed"; exit 1 ;;
  2) echo "CANNOT-MEASURE: the instrument or comparison was invalid"; exit 2 ;;
  *) echo "CANNOT-MEASURE: unexpected gate exit $code"; exit 2 ;;
esac
```

In the project that integrates the gate, store the per-item records and manifest as CI
artifacts on every outcome. A red job without the records cannot distinguish a product
regression from a broken gauge. The skill repository's own workflow tests the utilities;
it does not produce system-under-test run artifacts.
