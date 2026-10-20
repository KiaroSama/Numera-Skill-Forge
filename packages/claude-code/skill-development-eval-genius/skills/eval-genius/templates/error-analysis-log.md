# Error-analysis log

Fill this while reading real traces (`references/12-error-analysis.md`). It has three
parts: annotate every trace, roll the notes into categories, then turn each category into
a promise. The three tables paste straight into a spreadsheet; a domain expert can do all
of it with no eval vocabulary.

- **Source of traces:** (production logs / support tickets / beta / hand-written)
- **Author / date:**
- **How many traces read:**

## 1. Annotation log (one row per trace, free-form first)

Do not sort into categories yet. Just say, in plain words, what went wrong. Leave
`Category` blank on the first pass and fill it in Part 2.

| Trace id | Input (short) | What the system did wrong (plain words) | Bad? (y/n) | Category |
|---|---|---|---|---|
| | | | | |
| | | | | |
| | | | | |

## 2. Categories (roll the notes into named failure modes)

Cluster the annotations into a short list. Count how many of the traces you read hit each
one. Rank by impact (what it costs and to whom), not by frequency alone.

| Category name | What it is (one line) | Count / traces read | Impact (who it hurts, what it costs) | Rank |
|---|---|---|---|---|
| | | / | | |
| | | / | | |

## 3. Turn each category into a promise (hand off to `01-foundation.md`)

One row per category worth measuring, highest rank first. Each becomes a pre-registration.

| Category | Promise it becomes ("the system does not…") | Code-checkable? (y/n) | Grader it points to (`02` / `03` / `10`) |
|---|---|---|---|
| | | | |
| | | | |

## Output of this file

A ranked list of named error categories, each with a current per-error count and a
promise ready for `templates/preregistration.md`. Retire a category's eval once its count
reaches zero and stays there (`references/12-error-analysis.md`: don't aim for 100%).
