# Trigger evals: does the skill fire on the right prompts

The most common eval a skill, plugin, or router author ever runs, and the one
most often skipped. A skill that fires on the wrong prompts interrupts work it
was never meant to touch; one that misses the right prompts is dead code its
author believes in. Both failures are measurable, and both are measured the
same way: firing is a binary classification problem, and it gets a
classification eval.

## The unit under test is the description

The router does not read the skill; it reads the skill's frontmatter, mostly
the `description`. That makes the description the lever, and it is the only
lever: when activation is wrong, the fix is the description text, never the
test. One lever per comparison, as always: change the description, freeze the
suite, the prompts, and everything else.

## Build a balanced suite

Two halves, both required:

- **Should-trigger prompts.** The ways real users actually ask, including
  phrasings that never name the skill and jargon-free versions of the same
  need. Source them from real sessions and logs where possible; hand-written
  ones carry the author's imagined phrasing, not the user's.
- **Should-NOT-trigger prompts.** The negative space, and the half everyone
  forgets. Near-misses are where precision is measured: the query that shares
  vocabulary but wants a different tool, the request that is adjacent work
  rather than this skill's job. A suite without these can only ever measure
  recall.

Label every prompt with its expected verdict before running anything. Version
the suite like any fixture: it is data, it gets a hash, and a changed suite
means a new baseline.

## Score it as classification

Each prompt lands in one cell: fired or not, crossed with should or should
not.

| | Fired | Did not fire |
|---|---|---|
| **Should trigger** | correct | miss (false negative) |
| **Should not** | false alarm (false positive) | correct |

- **Precision:** of the prompts it fired on, the fraction that should have.
  Low precision means the description overclaims.
- **Recall:** of the prompts that should have fired, the fraction that did.
  Low recall means the description undersells, or the skill's real scope is
  narrower than its author thinks.

Report both, never F1 alone (`02-grading-and-metrics.md`). F1 is shorthand
after both numbers are on the table, because the two errors are not the same
price: a false positive costs the user a derailed turn, a false negative
means the capability might as well not exist. Which one hurts more is a
product call, and the pre-registration says which metric is primary.

## Read the cells, not just the rates

Every miss and every false alarm is a prompt to read. Ten misses that all
phrase the need the same way are a real signal about what the description
fails to say; ten scattered misfires on a noisy router are closer to
variance. Reproduce one by hand before trusting the count, the same rule as
every other eval.

## Iterate against a split, not the whole suite

Tuning the description against the full suite is gate-set tuning by another
name. Hold out a slice that is never used for iteration, evaluate the final
description on that slice, and report the held-out numbers. When a new
phrasing of the need shows up in the wild, it joins the suite, and the suite
version changes.

## Report activation with an interval

The activation rate is a single proportion k of n, so it gets a Wilson
interval, not a bare percentage:

```
scripts/rate_interval.py --k 13 --n 14     # fired on 13 of 14 should-trigger
scripts/rate_interval.py --k 1 --n 7       # false-fired on 1 of 7 should-not
```

"Fires on 13 of 14" and "fires on 13 of 14, interval [x, y]" are different
claims; only the second survives a reader who knows what n is. Small trigger
suites produce honestly wide intervals. Say so rather than rounding the
wobble away.

## The worked instance

This skill ships exactly such a suite as the running example. `evals/evals.json`
lists each user query with its expected verdict, 14 should-trigger and 7
should-not, mirrored one-to-one by the `evals/trigger-*` case directories and
kept in sync by a test. A third of the suite is negative space on purpose: a
description that overclaims fails there first, and an eval with no
should-not prompts could never catch it. When this skill's own description
changes, that suite is the paired diff that says whether the change helped.

## Output of this file

A versioned, balanced should-trigger / should-not suite; per-prompt cells
read before the rates; precision and recall reported separately with a Wilson
interval on each rate; the description iterated as the single lever against a
dev slice with a held-out slice untouched.
