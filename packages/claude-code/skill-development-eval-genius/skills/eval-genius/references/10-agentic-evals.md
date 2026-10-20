# Agentic evals: outcome, trajectory, reliability

Agents produce a trajectory (tool calls, intermediate states, messages) and an
outcome (the final state of the world). The two are graded separately, because many
correct trajectories reach one correct outcome, and a lucky trajectory can reach it
too.

## Outcome grading first

Grade the end state, deterministically:

- The resulting files, database rows, or API state match an annotated goal state.
- Hidden tests pass. Tests are hidden from the agent; leaked tests inflate.
- Nothing outside the task scope changed. Diff the world before and after; collateral
  damage is a failure even when the task passed.

The transcript is not consulted for the outcome verdict. That removes the temptation
to award credit for "trying hard."

## Trajectory grading second, and separately

Trajectory checks catch what outcome grading cannot:

- **Forbidden actions.** Destructive commands, out-of-scope writes, credential reads.
  Deterministic: pattern-match the tool log.
- **Efficiency.** Tool calls, tokens, wall time, retries. Deterministic.
- **Process quality.** Did it verify before claiming done, did it read before writing.
  Usually judged; calibrate per `03-judge-calibration.md`, and keep it out of the
  headline.

Report trajectory metrics next to the outcome metric, never blended into it.

## Reliability: pass^k beside pass@k

- **pass@k**: at least one of k independent trials succeeds. Rises with k. Measures
  capability.
- **pass^k**: all k trials succeed. Falls with k. Measures reliability, which is what a
  user experiences when they run the agent once and trust it.

An agent with pass@1 near 60% can have pass^8 under 25%. Report both. A gate on a
production agent uses pass^k with a pre-registered k.

## Sandboxing and infrastructure failure

- Every trial runs in a fresh, isolated environment with a pinned image. State leaking
  between trials is fixture drift.
- Setup timeouts, image pull failures, and container crashes are CANNOT-MEASURE for
  that item, recorded in the error field, never scored as a fail. A harness that
  reports 0% on a task the agent never entered is measuring infrastructure.
- Per-trial timeout is pre-registered and usually counts as a failure, because a user
  would experience it as one. Say which.

## Environment-contract preflight: test the world before the agent

The environment is part of the instrument. Prove it before any subject run:

- **Seed/reset determinism.** Same seed, identical world: run the reset twice
  and diff the state. A world that drifts between resets makes trials
  incomparable.
- **Tool contracts.** Each tool behaves as specified, including its failure
  modes; a tool that silently succeeds hands the agent a shortcut it did not
  earn.
- **Reachable success states.** The goal state is achievable from the seed.
- **No shortcut paths.** No route reaches the goal state without doing the
  task: an unguarded file, a default credential, a world that starts already
  solved.
- **Known-good passes, known-bad fails.** A reference trajectory passes and a
  deliberately broken one fails, in this environment, before the subject is
  ever run.

An environment that fails any of these ends the run CANNOT-MEASURE; the
verdict belongs to the world, not the agent. The discipline is the one
OSWorld-style seeded, resettable environments encode: borrow the
reset-and-success-check contract, not their worlds.

## The harness is a lever

Two agents on the same model with different scaffolds score differently. When the
comparison is between models, freeze the scaffold, tool set, budgets, and timeouts.
When the comparison is between scaffolds, freeze the model. Moving both is the
two-lever mistake from `01-foundation.md`.

## Items for agent tasks

Follow `05-dataset-construction.md`, with the four defect classes applied hard: an
over-strict hidden test fails a correct alternative solution; an under-specified prompt
makes the hidden test a guessing game. Every task gets a known-good reference solution
that passes and a known-bad one that fails, both run in the harness before the task is
admitted.

## Multi-turn and simulated users

When the task needs a user in the loop, the simulated user is part of the fixture:
pinned model, pinned persona script, pinned seed. A simulated user that changes
between runs is a control that moved. Grade the outcome state, not the conversation's
tone, unless tone is the promise.

The unit of measurement is the session, not the turn. Start from whether the
whole conversation achieved the intended user outcome; per-turn scoring
atomizes the dialogue and rewards local polish over task completion. A
turn-local claim ("responses are polite") may be graded per turn; a
session-success claim may not be assembled from isolated turns.

Fixture integrity for a session item:

- the full ordered transcript, every turn, nothing summarized away;
- the stop/end reason and the terminal state it produced;
- the intended outcome, written before the run;
- the user or scenario stratum the session was drawn from;
- the tool events, aligned to the turns that issued them.

Stratified coverage comes before any aggregate session claim: "success across
sessions" means across the strata the fixture declares, and a claim measured
in one stratum says nothing about another.

## Output of this file

An agent eval with deterministic outcome grading on end state, separate trajectory
metrics, pass^k beside pass@k with pre-registered k, isolated per-trial sandboxes,
infrastructure failures recorded as CANNOT-MEASURE, a frozen scaffold when the
model is the lever, a proven environment contract before the subject runs, and
session claims graded at the session, not the turn.
