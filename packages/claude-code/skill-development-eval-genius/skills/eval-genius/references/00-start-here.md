# Start here: do I need an eval, and where does it go?

For the reader who has never built one. Read this when the ask is "I think I might
need evals", "where do evals fit in my process", or "which eval should I run". It ends
with a concrete first-eval recipe; everything else in the skill deepens one step of it.

## Where to start, by who you are

- **Domain expert or PM, no eval background.** You own "what good looks like". Start from
  real failures, not theory: `12-error-analysis.md` turns your product's actual bad outputs
  into the things worth measuring. You do not need to know the vocabulary to begin.
- **Engineer building the thing.** Use the stage table below to place the first eval, then
  the first-eval recipe. The rest of the skill deepens each step.
- **Comparing systems or making a public claim.** You are in benchmark territory:
  `04-search-vs-build.md` first (adopt before you build), then `05` and `09`.

## What an eval is, in one paragraph

An eval is a repeatable check on a system whose outputs vary: a fixed set of inputs,
a written rule for what a good output looks like, and a score (usually a pass rate).
It is the test suite for the part of the product a unit test cannot pin down, because
a model, prompt, or retriever produces different text on different days. Where a
unit test says "equal or not", an eval says "how often, and is that enough".

## Do I need one? Three questions

1. **Does the output vary?** A model, a prompt, a retriever, a ranking, anything
   you will keep tuning. No: ordinary tests, stop here.
2. **Will you change it again and want to know if the change helped?** Or would a
   quiet regression cost you something (a wrong answer to a user, a broken agent
   action, a public number that turns out false)?
3. **Is a decision or a claim coming?** Model A vs B, ship or hold, a number in a
   README or a pitch.

No to all three: spot check by hand and move on. Yes to any: an eval, sized to the
stakes. Most projects reach "yes" the first time a prompt change breaks something
that used to work.

## Is an eval even the right spend?

Before building the instrument, check that the problem wants a measurement and
not a repair. An obvious, frequent defect you already understand gets fixed
now, not eval'd: measuring what you already know spends budget with no decision
attached. Automate only when the behavior has clear good and bad examples,
stable criteria for telling them apart, and recurrence or regression risk worth
guarding. Record the call in one line:

- **FIX_NOW**: the defect is known, frequent, and cheap to repair; fix it and
  move on.
- **MEASURE**: the criteria hold and the behavior recurs; build the eval.
- **CANNOT-MEASURE**: no stable criteria, or no verifiable evidence surface
  (`01-foundation.md`); fix the surface or the product first.

## Where evals sit in a development lifecycle

| Stage | What you are doing | Instrument | Smallest useful version | Fine to skip when |
|---|---|---|---|---|
| **Exploring** | Trying prompts and models, nothing fixed yet | Spot check | 10 hand-picked inputs, eyeball the outputs | Always; nothing to measure yet |
| **First working version** | It basically works and you are about to iterate | Smoke eval | 20 to 50 real inputs, code-checked, one pass rate. **This run is the baseline.** | The prototype is being thrown away |
| **Changing one thing** | Prompt tweak, model swap, retrieval setting | Paired eval vs baseline | Same items through old and new, per-item diff, bar written before the run | The change is cosmetic and untestable |
| **Merging or shipping** | Deciding whether this goes out | Gate in CI | Held-out items, PASS / FAIL / CANNOT-MEASURE, a known-bad item that must fail | Personal prototype, no users |
| **Comparing or claiming** | Versus a rival, an older version, or a public number | Benchmark | Versioned dataset plus harness, interval on every number, written report | No comparison, no claim |
| **Running in production** | Real traffic, real drift | Monitor | The same per-item scorer on a sampled slice of live traffic | Not launched yet |
| **Guarding each response** | Checking output inline, before the user sees it | Guardrail | One fast check on the response path; a failure retries the generation or routes it to a repair call | Latency budget forbids it, or nothing harmful can ship |

Timing rule: build the first eval at **first working version**. Earlier, there is
nothing stable to measure; later, every change since is unattributable and the eval
is built under pressure. Each later stage reuses the same items and scorer, heavier
only in what surrounds them. Items inside a suite have a lifecycle too: a
capability item that saturates graduates into the regression suite
(`07-gates-and-ci.md`).

## Which eval do I run? Forks

0. **What is the unit under test?** Name it before anything else, because it sets the
   scope of everything below.
   - **One step in the product** (a summarizer, a classifier, a retrieval call): a scoped
     eval on that step's inputs and outputs. Smallest, start here if unsure.
   - **The whole AI system end to end** (a multi-step agent, a full pipeline): a
     task-success eval on the final result, plus trajectory (`10-agentic-evals.md`); the
     harness is itself a lever.
   - **A comparison or public claim** (versus a rival, an older version, a number you will
     publish): a benchmark (`04-search-vs-build.md`), adopt before you build.
1. **What does one output look like?** An exact answer (a label, a number, a
   file), a structured object (JSON, a table), free text (a summary, a reply), or a
   sequence of actions (an agent). Name it; the grader follows from it.
2. **Can code check it?** Exact answers, structured objects, and end states of an
   agent: yes, and that is the whole grader. Free text: decompose first (required
   facts present, forbidden content absent, length and format) and only the
   remainder needs a model or human judge (`02-grading-and-metrics.md`).
3. **What is the metric?** Pick from the family that matches the promise: retrieval
   wants recall@k, classification wants precision and recall, generation wants fact
   coverage plus a judged layer, agents want task success plus trajectory
   (`02-grading-and-metrics.md`). Always record cost and latency next to it.
4. **How many items?** 20 to 50 to start; enough to see a change of the size that
   matters before trusting a delta (`08-statistics.md`). Include items where the
   right output is "refuse" or "nothing found".

Any existing public benchmark that measures the same promise beats building one
(`04-search-vs-build.md`); most first evals are private and small, and that is fine.

## The first-eval recipe (one afternoon)

1. **Collect 30 real inputs.** From logs, support tickets, or hand-written cases that
   look like real use. Five of them where the correct behavior is to refuse or return
   nothing.
2. **Write the expected result per item in a form code can check.** A string, a set
   of required facts, a schema, a final state. If an item resists, split it or park it
   for the judged layer later.
3. **Run the current system.** Save per-item results, not just the total. That number,
   with its n, is the baseline. Write down what version produced it.
4. **Write the bar before the next change.** "Accept if no item that passed now fails,
   and the pass rate is up by at least X." Also what would prove the change useless.
5. **Change one thing, run again on the same items, diff per item.** Read the result
   with `11-reading-results.md`.

Fill `templates/preregistration.md` with steps 3 and 4; it is the same five fields the
rest of the skill relies on, and a first-time user needs only the promise, the lever,
the baseline, and the bar.

## Output of this file

A yes/no on whether an eval is needed (FIX_NOW / MEASURE / CANNOT-MEASURE), the
stage it belongs to, the instrument for that stage, the grader type and metric
family, and a baseline run with per-item results. Nothing downstream needs more
than that to start.
