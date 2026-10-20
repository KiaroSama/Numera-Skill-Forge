# Walkthrough: a product step, first eval to verdict (beginner)

The ask, verbatim: "We rewrote the summarizer prompt. Is it better?"

This file walks the whole loop once, loading the same references a real session
would, in order. Every number below is real: it comes from running the shipped
scripts on the toy fixtures in `fixtures/`, and you can reproduce it exactly.
The fixture is toy-scale so the commands run in a second; a production version
is the same shape with more items.

## Step 0: is there an eval here at all (`../00-start-here.md`)

Three questions. Does the output vary: yes, it is a model on a prompt. Will it
change again: yes, this is the second prompt revision and not the last. Is a
decision coming: yes, ship v2 or keep v1. Stage on the table: **changing one
thing**, so the instrument is a paired eval against a baseline on the same
items.

If the ask had instead been "users complain the summaries are off and I don't
know where to start," the door would be `../12-error-analysis.md`: turn the
complaints into named error categories first, then each category becomes the
promise below. Not needed here; the promise is already clear.

## Step 1: pre-register (`../01-foundation.md`, `../../templates/preregistration.md`)

Filled before any run:

- **Promise:** the summary keeps every decision and drops the chatter.
- **Lever:** the system prompt, v1 to v2. One lever. Model snapshot, fixture,
  seeds, and item order are controls and stay frozen.
- **Outcome (primary):** deterministic check pass rate on the fixture.
- **Bar:** no per-item regressions, and pass rate up by at least +0.10.
- **Falsifier:** any regression, or a delta under +0.10, means v2 is rejected
  and v1 stays.
- **Baseline:** the current prompt run first, on this fixture.
- **Negative control:** `sum-neg`, a planted record whose output invents a
  refund amount. The scorer must fail it on every run.

## Step 2: the grader is code (`../02-grading-and-metrics.md`)

"Good summary" decomposes into checks code can run: every fact in the item's
`required_facts` list appears, no fact outside the source appears, the output
is at most three bullets, and `expect_refusal` items get a refusal, not a
summary. All four are deterministic, so there is no judged layer in this eval
at all. Item `score` is the fraction of checks passed (a fail can be 0.5, not
just 0); the gate verdict stays binary.

## The fixture, frozen and hashed

Thirty real-shaped inputs plus the plant: `fixtures/summarizer-items.json`,
including five items where the right answer is to refuse (empty body,
attachment-only, gated link, greeting, unsupported language). Hash it once,
put the hash in every run manifest:

```
$ python3 scripts/hash_fixture.py references/walkthroughs/fixtures/summarizer-items.json
sha256:5c90dcb5d3b7aeb714505a4097555d1294d211b8b6b919a8b57154ef1bb32996
```

## Run baseline, change one thing, run treatment

`fixtures/summarizer-baseline.json` is the v1 run, committed before v2 was
tried. `fixtures/summarizer-treatment.json` is the same 31 items through v2.
The treatment manifest asserts liveness ("passed"): the response metadata
showed prompt v2's hash, so the arm under test was really live.

```
$ python3 scripts/check_gate.py --baseline references/walkthroughs/fixtures/summarizer-baseline.json --treatment references/walkthroughs/fixtures/summarizer-treatment.json
fixture sha256:5c90dcb5d3b7aeb714505a4097555d1294d211b8b6b919a8b57154ef1bb32996  items 30  errors baseline 0 treatment 0
pass rate  baseline 0.7333  treatment 0.8667  delta +0.1333
improved 5  regressed 1  held 24
regressed ids: sum-08
FAIL: 1 regressions > allowed 0
```

```
$ python3 scripts/paired_bootstrap.py --a references/walkthroughs/fixtures/summarizer-baseline.json --b references/walkthroughs/fixtures/summarizer-treatment.json --reps 5000 --seed 0
n 31 items, resampled over 31 items, 5000 reps, seed 0
mean delta (b - a) +0.1452   95% interval [+0.0242, +0.2742]
improved 7  regressed 2  held 22
interval excludes zero on this fixture (still subject to fixture validity and label error)
```

## Read it in order (`../11-reading-results.md`)

1. **Did it run.** Exit 1, a clean FAIL: fingerprints match, zero errors, the
   plant failed on both runs. The comparison is valid; the verdict is real.
2. **Against the bar.** Two clauses: "+0.10 or better" passes (+0.1333), "no
   regressions" fails on `sum-08`. The bar was written before the run, so it
   bites now.
3. **Bigger than noise.** The bootstrap interval excludes zero, and the
   score-level read adds texture the binary gate cannot show: `sum-07`
   climbed 0.25 to 0.75 and `sum-26` slipped 0.5 to 0.25 without flipping
   verdicts. (The control record rides along at delta 0; it does not move the
   mean.) The improvement is real. So is the regression.
4. **Items, not averages.** Open `sum-08` by hand: v2 merged the requested
   capability and the user's use case into one bullet, and the fact check
   correctly flagged the missing use case. A real regression, not a scorer
   artifact.

## Verdict

Reject v2 as written. Not "v2 is bad": it is a net win on 30 of 31 items and
the interval says the gain is not noise. The bar said no regressions, and
there is one. The honest options are the ones from `07-gates-and-ci.md`: fix
the prompt and re-run on the same fixture, or accept with a written reason and
a baseline promotion. Shipping it silently is not on the list.

## The report (`../../templates/eval-report.md`, filled)

```markdown
## 1. Pre-registration (quoted verbatim)
> prereg-toy-summarizer: promise "the summary keeps every decision and drops
> the chatter"; lever prompt v1->v2; bar "no per-item regressions and pass
> rate up at least +0.10"; falsifier "any regression or delta under +0.10 ->
> reject"; baseline = current prompt on this fixture.

## 2. Runs
| Arm | Run id | Fixture hash | Liveness | Outcome | Errors |
| baseline | r-toy-sum-base | sha256:5c90...2996 | not-applicable | scored | 0/30 |
| treatment | r-toy-sum-treat | sha256:5c90...2996 | passed | scored | 0/30 |
Negative control sum-neg failed on both runs: yes.

## 3. Results (deterministic layer; no judged layer)
pass rate 0.7333 -> 0.8667, delta +0.1333; bootstrap +0.1452 [+0.0242, +0.2742];
improved 5 / regressed 1 / held 24; n=30 (toy scale).

## 5. Decision against the bar
REJECT. Delta clause met, no-regressions clause failed on sum-08 (real
regression, confirmed by hand).

## 6. Caveats
Toy-scale fixture; self-run; one regression unexplained by cost or scorer
defect. Falsifier not triggered for uselessness, triggered for the clause.

## 7. Tiers
All numbers measured on the shipped fixture; nothing estimated or aspirational.

## 9. Next
Fix the v2 bullet-merging behavior, re-run on the same fixture hash. If the
team accepts the regression with a written reason, promote the baseline in a
separate commit.
```

## Output of this file

One full loop, start to finish: stage, pre-registration, deterministic grader,
frozen and hashed fixture, baseline and treatment runs, a gate FAIL read
against the written bar, a bootstrap interval, a hand-checked regression, and
a filled eval-report whose decision is reject-with-a-next-step. Every number
reproduces from `fixtures/` with the commands shown.
