# Error analysis: finding what to measure when you have failures, not a promise

The other front door. `01-foundation.md` starts top-down from the promise a system
makes. This file starts bottom-up from what the system actually got wrong. Read it when
the user has a working system and a pile of bad outputs but cannot yet say what "good"
means in the abstract: "the answers are sometimes off and I don't know where to start",
"how do I even know what to test", "our users complain but I can't reproduce it". This is
the part a domain expert or PM owns, and it needs no eval vocabulary to begin.

The two doors meet in the same room. An error category **is** a promise, stated from the
other side: "the system does not produce output of failure-class X." Error analysis is
how you discover promises from evidence; pre-registration (`01-foundation.md`) is where
each one gets written down and measured. So this file is the front half of the
foundation, not a detour around it.

## Why start from errors

You cannot measure quality in the abstract. "Is the summary good" has no answer until you
know the ways summaries go bad *in this product*: dropped decisions, invented facts, wrong
tone. Correctness is use-case specific. An off-the-shelf eval measures the errors its
author saw, not the ones your users hit, so a tool's built-in metrics are a starting
checklist at best and a distraction at worst. If you have not looked at your own failures,
you do not yet know what to measure, and any metric you pick is a guess.

## The loop

1. **Gather real traces.** Actual inputs and outputs from real or realistic use: logs,
   support tickets, a beta shipped specifically to produce traces. Real traffic beats
   synthetic examples, which carry the failures you imagined rather than the ones you have.
   Fifty traces is enough to start; you are looking for patterns, not coverage.
2. **Read and annotate, free-form first.** Go through traces by hand and write, in plain
   words, what went wrong with each bad one. Do not pre-sort into categories; let the notes
   be messy ("missed the refund step", "answered a different question", "right answer,
   rude tone"). This is open coding: name failures before you group them.
3. **Categorize into failure modes.** Cluster the free-form notes into a short list of
   named error classes. Most systems have a handful that cover the bulk of the damage.
   The list is yours and specific; it will not match any generic rubric.
4. **Prioritize by impact, not frequency alone.** A rare failure that loses a customer
   outranks a common cosmetic one. Use the same judgment you would use to triage any
   product problem: what does this error cost, and to whom.
5. **Write one eval per error category.** Not one eval per quality dimension, one per
   *error*. An eval here is a counter for a single specific failure: "of 105 suggested
   questions, how many were leading?" Push it to code wherever the failure is
   code-detectable (`02-grading-and-metrics.md`); reserve a judge for the classes only a
   human can spot, and calibrate it (`03-judge-calibration.md`).
6. **Report per-error counts, never one blended quality score.** "Leading question:
   15/105; dropped-decision: 3/40" tells you what to fix. A single "82% good" hides which
   error is doing the damage and gives you nothing to act on.

Work the whole loop in `templates/error-analysis-log.md`: annotate traces, roll them into
ranked categories, and turn each category into a promise. It is spreadsheet-shaped so a
domain expert can fill it with no eval vocabulary.

## Taxonomy provenance: human first, machine second

Open coding is a human job first. A domain expert reads the first 30 to 50
traces and names the failures in free text, no model in the loop. Only after
that seed exists may an LLM help: proposing clusters over the human's notes,
normalizing wording, mapping later annotations into the existing classes,
always under human review. The machine accelerates the taxonomy; it does not
author it.

Record the provenance so the taxonomy can be audited:

- the open-coded seed count and the domain owner who produced it;
- the raw notes, kept verbatim;
- each proposed machine cluster and the human acceptance or edit;
- a taxonomy version, bumped whenever a class changes;
- an **OTHER / NEW_FAILURE** escape hatch on every later annotation, so a
  failure that fits no class is captured, never forced into one.

An LLM-first or LLM-only taxonomy cannot support completeness or prevalence
claims: the classes were found by a model with its own blind spots, and "the
model saw no other failure" is not evidence none exists. Claims of the form
"these classes cover the failures" or "class X is N% of failures" resolve to
CANNOT-MEASURE until a human-open-coded seed exists.

For opt-in typed-question classification after the human taxonomy is frozen, see `15-decision-model-judge.md` for confidence routing, cascade cost, and the earn-it agreement floor.

## Two things the loop gives you for free

- **Writing the eval sharpens the definition.** You will discover that your intuition for
  an error was fuzzy until you had to write the rule that counts it. The act of making the
  failure measurable clarifies what the failure even is; the rule often ends up better than
  the mental model you started with.
- **The loop never ends, and that is the point.** Fixing one error class routinely surfaces
  a new one your evals do not yet track. A permanent stream of hand-labeled traces is how
  you detect failure modes you have not thought of; a system that only runs the evals it
  already has is blind to its next error. Budget for continued reading, not just for the
  suite you have.

## Do not aim for 100%

An error-detector eval that reads 100% is measuring nothing: the error is no longer in
your data. That is not a trophy, it is a signal to retire or refresh the eval and spend
the attention on a live error class (`09-reporting.md` on retirement). A capability you
are still climbing toward starts low on purpose; an error you have beaten leaves the
active suite. Neither should sit at a permanent 100% pretending to guard something.

## Handing off to the rest of the skill

Each prioritized error category becomes a promise and walks into the foundation:

| Error category (from your traces) | The promise it becomes (`01-foundation.md`) | Grader it points to (`02`) |
|---|---|---|
| "Invents facts not in the source" | "Every claim in the summary is grounded in the source" | Fact-coverage + forbidden-fabrication check |
| "Skips the refund step" | "The agent completes every required step of the task" | End-state / trajectory check (`10`) |
| "Leading interview questions" | "Suggested questions are open, not leading" | Calibrated judge on a labeled slice (`03`) |

From there the normal chain applies: pre-register the promise and bar (`01`), pick the
grader (`02`), assemble items including the negative space (`05`), and read results item
by item (`11`). Error analysis is not a replacement for that discipline; it is how a
first-timer finds the promises worth putting through it.

## Output of this file

A short, prioritized list of named error categories drawn from real traces with
recorded human-first provenance, each with a per-error count from the current system
and each restated as a promise ready for `01-foundation.md`. That list is the agenda
for every eval that follows.
