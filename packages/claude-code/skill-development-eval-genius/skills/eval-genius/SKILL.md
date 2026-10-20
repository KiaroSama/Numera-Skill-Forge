---
name: eval-genius
description: >-
  Decide whether an AI/LLM/agent/retrieval system needs an eval, where it fits in the
  dev process, which one to run, and how to read the result; then design, gate, judge,
  and defend it. Not ordinary unit tests.
---

# Eval Genius

An eval is a claim you are willing to defend under hostile audit. You measure to earn the right to say "this is better" and have it hold when someone sharp pushes back.
Behave like a measurement engineer: state the promise, fix the bar before looking, hold
everything else constant, distrust the instrument first, report the number that hurts.

---

## Step 0: Does this need an eval, and where does it go?

Three questions decide it (`references/00-start-here.md`): does the output vary? will it
change again, and would a quiet regression cost something? is a decision or public claim
coming? No to all: a spot check, stop. Yes to any: an eval, sized to the project's stage.
Two checks sit in front. Preflight: a domain expert can verify the output without
redoing the work, else CANNOT-MEASURE (`references/01-foundation.md`). Triage: a
known, frequent defect is fixed now, not measured (FIX_NOW / MEASURE / CANNOT-MEASURE).

| Stage the user is at | Instrument | Smallest useful version |
|---|---|---|
| Exploring prompts and models | Spot check | 10 inputs, eyeball |
| First working version | Smoke eval | 20 to 50 real inputs, code-checked; **this run is the baseline** |
| Changing one thing | Paired eval vs baseline | Same items both arms, per-item diff, bar written first |
| Merging or shipping | CI gate | Held-out items, three-way outcome, a known-bad item that must fail |
| Comparing or claiming publicly | Benchmark | Versioned dataset and harness, intervals, report |
| In production | Monitor | Same scorer on sampled live traffic |

Build the first eval at "first working version", never before, rarely after. For a first-timer, run the one-afternoon recipe in `00-start-here.md` and touch nothing else.

## Route the request

Identify the job, then load only that reference. Every job still passes through Step 1.

| User needs to... | Load |
|---|---|
| Know if they need an eval, where it fits, which one, or how to start | `references/00-start-here.md` |
| Start from real failures or bad outputs, or "what should I even measure" | `references/12-error-analysis.md` |
| Decide what to measure at all, or the ask is "make it better" | `references/01-foundation.md` |
| Pick a grader or metric for a task | `references/02-grading-and-metrics.md` |
| Use, prompt, or trust an LLM judge | `references/03-judge-calibration.md` |
| Consider an opt-in decision-model judge for binary/closed-label residue, with or without a provider key | `references/15-decision-model-judge.md` (examples: `jev_cascade_cost.py`, `jev_confidence_route.py`, `jev_choice_route.py`, `jev_calibrate.py`, `jev-lane-preregistration.md`) |
| Choose between an existing benchmark and a custom one | `references/04-search-vs-build.md` |
| Assemble items, labels, negatives, splits; contamination, overfitting | `references/05-dataset-construction.md` |
| Write or fix the runner, scorer, or reporter | `references/06-harness-design.md` |
| Put an eval in CI or a release gate | `references/07-gates-and-ci.md` |
| Say whether a delta is real | `references/08-statistics.md` |
| Write results up, or retire a benchmark | `references/09-reporting.md` |
| Evaluate an agent, tool use, or multi-turn task | `references/10-agentic-evals.md` |
| Read a result file with no prior experience | `references/11-reading-results.md` |
| Evaluate my own skill or plugin: does it fire on the right prompts | `references/13-trigger-evals.md` |
| Test whether the system resists attack, or defend a safety claim | `references/14-safety-threat-model.md` |
| Follow a full worked example, start to finish | `references/walkthroughs/` |

Templates in `templates/` are copied into the project, never edited in place. Scripts in
`scripts/` are stdlib-only `--help` CLIs, nonzero on failure: `check_gate.py` (per-item
diff, exits 0/1/2 = PASS/FAIL/CANNOT-MEASURE, refuses fingerprint mismatch),
`paired_bootstrap.py` (paired delta interval, cluster-aware), `judge_agreement.py`
(kappa + PASS precision/recall vs humans), `rate_interval.py` (Wilson interval),
`hash_fixture.py` (fixture hash). Load references on demand, not all at once.

---

## Step 1: Foundation (five minutes, never skipped)

Copy `templates/preregistration.md` next to the fixture and fill it before touching data or code. A first-timer fills promise, lever, baseline, and bar; the rest follows.

1. **Promise.** One plain sentence: what does the system promise, what is "better"?
2. **Variables.** Levers, outcomes, controls; one lever per comparison.
3. **Placement.** A decision point, a risky seam, or a public claim; elsewhere, a spot
   check or nothing.
4. **Weight.** Spot check, eval, benchmark, or monitor (Step 0 table).
5. **Bar.** Threshold, falsifier, outlier rule. Written before any run.

## Step 2: Choose the grader (deterministic first)

Push every check that *can* be code-graded down to code: exact match, regex, schema,
test suite, threshold. Free text gets decomposed (required facts present, forbidden
content absent, format) before any judge sees it; reserve the judge for the residue.
80% deterministic / 20% judged is trusted; 100% judged is an opinion with error bars.
Report layers separately, never one blended number. A judge is an instrument:
calibrate against human labels, blind it, randomize order, pin model and prompt hash
(`references/03-judge-calibration.md`).

If binary/closed-label judged dimensions remain, suggest the optional hosted typed-question lane (`references/15-decision-model-judge.md`): probabilities route confidence, fixture text leaves the machine only after opt-in, and DISCOVERY_ONLY must earn the user's kappa-0.8 floor. If `TYPESAFE_API_KEY` is present, offer to use it; if absent, still suggest and offer to connect one. Never auto-call; if the user declines, keep today's path untouched.

## Step 3: Build or adopt

Search before building; an established benchmark buys ground truth nobody in the room
cooked. Build custom the moment the public one rewards a proxy the system does not
target, reusing public plumbing. Score candidates on
`templates/benchmark-assessment-scorecard.md`: what it rewards, contamination,
label-error ceiling, mechanism fit, maintenance.

## Step 4: Run under hard rules (a run that breaks one is not a result)

- **Freeze the fixture.** Same items, corpus, snapshot, seeds; only the lever varies.
  Refuse comparisons across mismatched fingerprints.
- **Three-way outcome.** PASS, FAIL, or CANNOT-MEASURE. A crash, missing baseline, or
  fingerprint mismatch is CANNOT-MEASURE, never FAIL and never PASS (`check_gate.py`).
- **Prove the treatment arm is live** before the run.
- **Cover the negative space**: items where the right answer is to refuse or return nothing.
- **Repeat and show spread.** Noise wider than the effect means no result yet.
- **Verify the verifier.** A known-bad case must go red before green is trusted.

## Step 5: Read the result (`references/11-reading-results.md`, in order)

1. **Did it run?** Exit code before score; negative control failed.
2. **Against the written bar**, not against hope. Above: candidate win. Below the
   falsifier: rejected. Between: not established.
3. **Bigger than noise?** Paired interval on the delta (`paired_bootstrap.py`);
   interval includes zero means "not established", never "no effect".
4. **Items, not averages.** Read regressions first; reproduce one flip by hand.
5. **Surprised?** A 0%, a 99%, a thirty-point jump is a harness bug until proven otherwise; fix the gauge, never the system, or stop.
6. **Layers and cost separately.** A win that doubled cost is a trade.

## Step 6: Report honestly
Numbers are claims with tiers, measured / estimated / aspirational, never summed. Three
sentences minimum: the bar and whether it was met; the delta with interval and flips; the
caveat that most weakens the claim. Self-run is said; failures retire in writing
(`templates/eval-report.md`).

---

## Anti-patterns (named so they can be refused)

- **Metric-first.** A dataset and scale chosen before the promise is written.
- **Eval too early or too late.** A prototype still in flux, or a shipped system with unattributed changes.
- **Blended score.** One number hiding which layer moved.
- **Post-hoc bar.** Threshold decided after the result is known.
- **Fixture drift.** Comparing across corpora, caches, or snapshots.
- **Cache blindness.** Reading a pre-built cache never exercises the write path.
- **Silent crash.** Harness error recorded as a score.
- **Oracle judge.** Uncalibrated model judge treated as ground truth.
- **Rubric-author bias.** Whoever built the system also wrote the rubric, alone.
- **Run until green.** Repeating a noisy eval until one run passes.
- **Gate-set tuning / overfitting.** Iterating on the held-out items the gate uses.
- **Grader gaming.** The subject passing by loophole, leaked test, or an exploited verifier; a gameable instrument makes the run CANNOT-MEASURE (`references/02-grading-and-metrics.md`).
- **Discovery metric as claim.** A ranking or outlier heuristic promoted to PASS/FAIL; discovery metrics never gate.
- **LLM-first taxonomy.** Failure classes proposed by a model before a human open-codes the traces; prevalence and completeness claims on it are CANNOT-MEASURE.
- **Safety-score blending.** One number across attack classes; report per class, and a safety claim with no audit trail is CANNOT-MEASURE.
- **Unverifiable output.** An eval built on outputs nobody can check without redoing the work; fix the evidence surface first.
- Run-completion checklist before calling anything done: `templates/quality-checklist.md`.
