# Judged-lane pre-registration

Filled before the first judged-lane run, next to `templates/preregistration.md`
(which stays the source of truth for the eval itself — this file covers only the
optional judged lane on top of it). The judge here is a decision-model judge: a
typed-question grader that returns a decision plus a confidence probability
`noul` in [0,1]. Everything below is decided before a `noul` value has been seen;
a change is a new pre-registration.

- **Id:** jev-prereg-YYYYMMDD-short-name
- **Author / date:**
- **Parent pre-registration id:**

## Judged-lane dimensions

Which rubric dimensions route to the judge instead of the deterministic checks.
One row each; a dimension not listed stays deterministic.

| Dimension | Why code cannot check it | Judge prompt/rubric version |
|---|---|---|
| | | |
| | | |

<!-- Example row (illustrative only): | answer-correctness | requires semantic reading of a typed question | Jev (TypeSafe) prompt v3, hash … | -->

## Calibration set

50–100 real labeled items, drawn from the same distribution the lane will see —
not synthetic, not cherry-picked.

- **Size and source:**
- **Who labeled it, and the labeling rule:**
- **Held-out slice size (judge never sees during rubric iteration):**
- **Fixture hash (`hash_fixture.py`):**

## Agreement gate

The judge counts as a grader only if judge-vs-human agreement clears the bar on
the held-out slice.

- **Tool:** `scripts/judge_agreement.py --human <file> --judge <file>`
- **Bar (default: Cohen's kappa ≥ 0.8):**
- **What happens if it fails:** (judge becomes a diagnostic, not a grader — fix
  rubric or guideline and re-run on the held-out slice)

## Routing thresholds

Chosen from the calibration above, not from a library default. `noul` strictly
above HIGH auto-accepts, strictly below LOW auto-rejects, and everything between
— including items exactly on either boundary — escalates. Record what the
escalate bucket costs in reviewer time, not just judge tokens.

| Threshold | Value | Basis in your calibration (what you measured) |
|---|---|---|
| LOW (auto-reject below) | | |
| HIGH (auto-accept above) | | |

- **Router:** `scripts/jev_confidence_route.py --low <LOW> --high <HIGH>`
- **Escalate bucket goes to:** (human review / slower lane / dropped with disclosure)

## Projected cost

Judge-side spend is linear in the residue rate — the fraction of items the cheap
deterministic filter escalates. Measure the residue rate and tokens per item on
a real slice first.

| Input | Value | Where it came from |
|---|---|---|
| items per period | | |
| residue rate | | measured on a real traffic slice |
| tokens per judged item | | |
| price per billion input tokens | | judge vendor's current price |
| projected cost | | `scripts/jev_cascade_cost.py --items … --residue-rate … --tokens-per-item … --price-per-btok …` |

## Falsifier for the lane

The result that would show the judged lane is not worth its cost or its
escalations — e.g. the escalate bucket is so large the lane saves nothing, or
kappa falls and stays below the floor on re-calibration.

>

## Output of this file

A judged-lane section of the eval's pre-registration: which dimensions the judge
owns, proof it agrees with humans at kappa ≥ 0.8 on a held-out slice, the two
routing thresholds and their measured basis, and the projected judged-lane cost.
Committed before the first judged run and quoted in the report, same as the
parent pre-registration.
