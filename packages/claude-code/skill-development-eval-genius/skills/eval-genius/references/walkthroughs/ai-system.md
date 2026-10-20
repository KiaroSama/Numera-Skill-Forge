# Walkthrough: an end-to-end agent eval

The ask, verbatim: "We swapped the agent's planner scaffold. Is the support
agent better now?"

Same loop as `product-step.md`, bigger surface: the unit under test is a whole
agent, so the harness carries more weight and the reliability question is
different. Same rule applies throughout: every number shown is produced by the
shipped scripts on the toy fixtures; run them.

## Step 0: which eval (`../00-start-here.md`)

The unit under test is the whole AI system end to end: a multi-step agent
whose intermediate calls matter but whose verdict is the final state of the
world. That means a task-success eval with trajectory on the side
(`../10-agentic-evals.md`), sized as a paired comparison against the current
scaffold.

## Step 1: the harness is a lever (`../01-foundation.md`)

- **Promise:** the agent completes the customer's task without breaking
  anything it did not touch.
- **Lever:** the planner scaffold, v1 to v2. The model is frozen. Two agents
  on the same model with different scaffolds score differently, so when the
  scaffold is the lever, the model snapshot is a control
  (`../10-agentic-evals.md`).
- **Outcome (primary):** task success = the end state matches the annotated
  goal state, graded in code. Secondary: per-task trial fraction, trajectory
  checks, cost.
- **Bar:** pass^4 rate up by at least +0.10 with no per-task regressions.
- **Falsifier:** pass^4 flat or down, or any task that newly fails every
  trial, means the new scaffold is rejected.
- **Reliability pre-registration:** 4 trials per task; a task's verdict is
  pass only if all 4 trials succeed (pass^4). pass@4 is reported alongside
  but does not gate anything.

## The fixture (`../10-agentic-evals.md`, `../05-dataset-construction.md`)

Twelve tasks in `fixtures/agent-tasks.json`, each with an annotated goal
state, plus `task-neg`, a planted record whose goal demands something policy
forbids; the scorer must fail it every run. Every task carries a known-good
reference trajectory and a known-bad one, both exercised through the harness
before the task was admitted. The simulated user is part of the fixture:
pinned persona script, pinned model, pinned seed. A user that changes between
runs is a control that moved.

## Mock the world with contract-checked fakes (`../06-harness-design.md`)

The agent calls an order lookup and a refund API. Both are stubbed with
recorded responses, and each stub asserts what the agent *asked*: a refund
call with the wrong argument shape aborts the run instead of returning a
polite fake. Each trial runs in a fresh sandbox with a pinned image; state
leaking between trials is fixture drift.

An infrastructure failure is not a fail. `fixtures/agent-treatment-flaky.json`
is a run where one sandbox died at setup:

```
$ python3 scripts/check_gate.py --baseline references/walkthroughs/fixtures/agent-baseline.json --treatment references/walkthroughs/fixtures/agent-treatment-flaky.json
CANNOT-MEASURE: 1/12 treatment items errored (8.3%) > error budget 2.0%. Fix the harness; a run with holes is not a scored run.
```

Exit 2, not exit 1. The agent never entered that task, so a 0 would have been
a lie about the system rather than a measurement of it.

## The judged layer earns its place (`../03-judge-calibration.md`)

Trajectory quality ("did it verify before claiming done") is judged, not
asserted. The judge was calibrated against human labels on 50 held-out
trajectory slices before it was allowed to grade anything:

```
$ python3 scripts/judge_agreement.py --human references/walkthroughs/fixtures/agent-human-labels.json --judge references/walkthroughs/fixtures/agent-judge-labels.json
n 50 shared items
coverage  human 1.000   judge 1.000
human positive rate 0.500   judge positive rate 0.500
raw agreement 0.920   (inflated by class imbalance; do not use for the decision)
Cohen's kappa 0.840   floor 0.8
judge PASS precision 0.920   recall 0.920   (vs human PASS)
disagreements (first 20): j-03, j-17, j-31, j-44
OK: judge meets the floor on this set; pin model + prompt hash in the manifest.
```

Kappa 0.84 clears the 0.8 gate floor. The four disagreements were inspected:
two rubric ambiguities, fixed in the guideline; the judge stays blinded,
order-randomized, pinned by model snapshot and prompt hash.

## The comparison, honestly

```
$ python3 scripts/check_gate.py --baseline references/walkthroughs/fixtures/agent-baseline.json --treatment references/walkthroughs/fixtures/agent-treatment.json
fixture sha256:fab18222ed8e02a002c6c6b6ddd3b711cf376a4f75849966fe0606693f481104  items 12  errors baseline 0 treatment 0
pass rate  baseline 0.6667  treatment 0.8333  delta +0.1667
improved 2  regressed 0  held 10
PASS
```

```
$ python3 scripts/paired_bootstrap.py --a references/walkthroughs/fixtures/agent-baseline.json --b references/walkthroughs/fixtures/agent-treatment.json --reps 5000 --seed 0
n 13 items, resampled over 13 items, 5000 reps, seed 0
mean delta (b - a) +0.0577   95% interval [-0.0769, +0.2500]
improved 2  regressed 2  held 9
interval includes zero: not distinguishable from noise on this fixture
```

```
$ python3 scripts/rate_interval.py --k 10 --n 12
k 10  n 12  point estimate 0.8333
95% Wilson interval [0.5520, 0.9530]
```

Read it straight: the gate metric (pass^4 per task) moved +0.1667 with zero
regressions, so the bar is met and the scaffold change is accepted. Two honest
caveats ride with it. The trial-fraction delta is +0.058 with an interval that
includes zero, so the finer-grained movement is not established at n=12; and a
pass^4 point estimate of 0.83 on 12 tasks carries a Wilson interval from 0.55
to 0.95, which is what small agent suites honestly look like. `task-07` and
`task-10` still fail every trial under both scaffolds; they are the agenda for
the next change, and per `12-error-analysis.md` their traces get read by hand.

## Where this eval lives (`../07-gates-and-ci.md`)

The outcome layer runs at gate tier on every pull request: deterministic
end-state checks, the negative control, the liveness assertion, minutes of
runtime. The judged trajectory layer and the 4-trial reliability runs move to
nightly, where their cost and noise belong. An infra flake gets one automatic
retry only because it ends CANNOT-MEASURE; a FAIL never retries.

## The report (`../../templates/eval-report.md`, filled)

```markdown
## 1. Pre-registration (quoted verbatim)
> prereg-toy-scaffold: promise "the agent completes the customer's task
> without breaking anything it did not touch"; lever scaffold v1->v2, model
> frozen; primary pass^4 per-task rate; bar "+0.10 and no regressions";
> falsifier "flat/down pass^4 or any newly-always-failing task -> reject";
> k=4 trials, per-trial isolation, simulated user pinned.

## 2. Runs
| Arm | Run id | Fixture hash | Liveness | Outcome | Errors |
| baseline | r-toy-agent-base | sha256:fab1...1104 | not-applicable | scored | 0/12 |
| treatment | r-toy-agent-treat | sha256:fab1...1104 | passed | scored | 0/12 |
Negative control task-neg failed on both runs: yes.

## 3. Results
Deterministic outcome layer: pass^4 0.6667 -> 0.8333, delta +0.1667,
improved 2 / regressed 0 / held 10, n=12; Wilson on treatment [0.5520, 0.9530].
Trial-fraction delta +0.0577 [-0.0769, +0.2500] (includes zero).
Judged layer (trajectory, nightly tier): judge kappa 0.84 vs humans on the
calibration set; prompt hash + model snapshot pinned.

## 5. Decision
ACCEPT the scaffold change: bar met on the pre-registered metric. The
trial-fraction movement is promising but not established; more trials on
task-07 and task-10 are queued, not claimed.

## 6. Caveats
Toy-scale n=12 makes the Wilson interval wide; self-run; pass@4 exceeds
pass^4 (reliability, not capability, is what the gate watches); one earlier
run was CANNOT-MEASURE on a sandbox failure and was re-run, not scored.

## 7. Tiers
Gate and calibration numbers measured; the trajectory-quality trend is
estimated (small judged sample).

## 9. Next
Baseline promotion in a separate commit with this manifest attached.
Hand-read task-07 and task-10 traces into the error log.
```

## Output of this file

A full agent eval: task fixture with goal states and a plant, contract-checked
fakes, per-trial isolation with infra failures refused as CANNOT-MEASURE, a
calibrated judge on the trajectory layer, a gate PASS read against the bar,
and a report that accepts the change while flagging the interval that does
not yet support the stronger claim.
