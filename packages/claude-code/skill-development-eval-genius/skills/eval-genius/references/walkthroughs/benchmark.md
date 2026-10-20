# Walkthrough: a benchmark behind a public claim

The ask, verbatim: "We want to say in the launch post that our help-center
retrieval finds the right article. Can we claim a number?"

A public claim raises the weight from eval to benchmark: versioned fixture,
labeled data, intervals on everything, and a report written for a hostile
reader. Same rule as the other walkthroughs: every number below is a real run
on the toy fixtures; reproduce it before believing it.

## Step 1: search before building (`../04-search-vs-build.md`)

The capability, named in field terms: single-document retrieval for support
questions. Three candidates went through
`../../templates/benchmark-assessment-scorecard.md`:

- **Candidate A, a well-known general QA set.** Scored fine until question 4:
  its items test encyclopedic recall, not "which help-center article answers
  this." It rewards a neighbor's mechanism. Rejected.
- **Candidate B, a support-ticket retrieval set.** Right mechanism, wrong
  corpus domain, and the last commit is two years old with open errata
  unanswered. A stale leaderboard offers no comparison and its label errors
  are now un-auditable. Rejected, noted as the stale candidate it is.
- **Candidate C, an open retrieval harness.** Good plumbing, ships no tasks
  that match. Kept as plumbing; tasks get built.

Decision: build the task set, reuse public harness plumbing, and label every
number self-run. That label is what keeps the claim credible.

## Step 2: the fixture is the claim (`../05-dataset-construction.md`)

`fixtures/retrieval-items.json`: 40 queries against the frozen help-center
corpus, stratified and manifest-carried: 20 how-to, 12 policy, 8
verified-absent. The absent stratum is the negative space: queries whose
answer provably is not in the corpus, where the right behavior is "no article
found" rather than a confident near-miss. Plus `ret-neg`, a plant whose gold
label points at the wrong document on purpose.

- **Labeling guideline first:** what counts as "the right article", what a
  partial answer is, worked examples of each verdict.
- **Two labelers on the whole set**, disagreements adjudicated with written
  reasons fed back into the guideline. At toy scale the agreement figure is
  illustrative; at real scale it is measured and carried in the manifest.
- **Splits:** a dev slice for iterating the retriever and a held-out slice
  the launch number comes from. The scores below are on the held-out slice.
- **Contamination:** the set is private, every item carries a creation date,
  and no item text goes into prompts, docs, or tickets that get crawled.

## Step 3: the harness refuses to lie (`../06-harness-design.md`)

Fixture, runner, scorer, reporter as separate parts; per-item records; a
manifest per run. The fixture hash is computed once and pinned:

```
$ python3 scripts/hash_fixture.py references/walkthroughs/fixtures/retrieval-items.json
sha256:66c898a60503b2be41f9fbe8730e9d249ddaaebd13228ff4d928723362a07926
```

The fingerprint refusal is not a nicety, it is the load-bearing check. Feed
the reporter a baseline and a treatment from different fixtures and it
refuses rather than compare:

```
$ python3 scripts/check_gate.py --baseline references/walkthroughs/fixtures/retrieval-v1.json --treatment references/walkthroughs/fixtures/summarizer-treatment.json
CANNOT-MEASURE: fixture fingerprint mismatch: baseline=sha256:66c898a60503b2be41f9fbe8730e9d249ddaaebd13228ff4d928723362a07926 treatment=sha256:5c90dcb5d3b7aeb714505a4097555d1294d211b8b6b919a8b57154ef1bb32996. Runs on different fixtures are not comparable; re-run both on one fixture.
```

## Step 4: the numbers, with intervals (`../08-statistics.md`)

Two runs on the frozen fixture: `retrieval-v1.json` (the retriever that
shipped last quarter) and `retrieval-v2.json` (the candidate for the launch
post). Verdict is pass when the right article is found; score is graded (1.0
found, 0.5 right neighborhood wrong article, 0.0 miss or bad abstention).

```
$ python3 scripts/paired_bootstrap.py --a references/walkthroughs/fixtures/retrieval-v1.json --b references/walkthroughs/fixtures/retrieval-v2.json --reps 5000 --seed 0
n 41 items, resampled over 41 items, 5000 reps, seed 0
mean delta (b - a) +0.0976   95% interval [-0.0122, +0.2195]
improved 6  regressed 2  held 33
interval includes zero: not distinguishable from noise on this fixture
```

```
$ python3 scripts/rate_interval.py --k 31 --n 40
k 31  n 40  point estimate 0.7750
95% Wilson interval [0.6250, 0.8768]
```

```
$ python3 scripts/rate_interval.py --k 5 --n 8
k 5  n 8  point estimate 0.6250
95% Wilson interval [0.3057, 0.8632]
```

Three claims, three honest shapes:

- **The headline level:** v2 answers 31 of 40 held-out queries, Wilson
  [0.625, 0.877]. Publishable as "about three quarters on our held-out set"
  with the interval printed next to it, never as a bare "78%".
- **The version delta:** +0.098 with an interval that includes zero. The
  launch post does not get to claim "v2 is better than v1". Not established
  is the finding, and it goes in the report, not in a drawer.
- **The negative space:** v2 abstains correctly on 5 of 8 absent queries, an
  interval too wide to brag about and a line the report shows anyway.

On sample size: this fixture is toy-scale by construction so the commands
run instantly. A real benchmark sizes the set from the minimum detectable
effect first; at n=40 the noise floor only clears for effects larger than
about a fifth of the scale, which is exactly what the bootstrap interval
just demonstrated.

## Step 5: the report is the deliverable (`../09-reporting.md`)

Tiers, disclosures, and the retirement rule all apply: measured numbers only,
self-run labeled as self-run, layers and strata separated, and a written note
on when this benchmark stops being useful (saturation, contamination, or a
promise that expired).

```markdown
## 1. Pre-registration (quoted verbatim)
> prereg-toy-helpcenter: promise "the retrieval finds the right help-center
> article for a support question, and says so when there is none"; primary
> per-item verdict rate on the held-out slice; secondary abstention accuracy
> on the absent stratum; comparison vs the currently shipped retriever.

## 2. Runs
| Arm | Run id | Fixture hash | Liveness | Outcome | Errors |
| baseline | r-toy-ret-v1 | sha256:66c8...7926 | not-applicable | scored | 0/40 |
| treatment | r-toy-ret-v2 | sha256:66c8...7926 | passed | scored | 0/40 |
Negative control ret-neg failed on both runs: yes.

## 3. Results (deterministic layer only)
Held-out verdict rate: v2 0.7750, Wilson [0.6250, 0.8768], n=40.
Paired delta v2 - v1: +0.0976 [-0.0122, +0.2195]; improved 6 / regressed 2 /
held 33 on scores; verdict flips +5/-1.
Absent stratum abstention: 5/8, Wilson [0.3057, 0.8632].
Per-stratum: how-to 18/20, policy 10/12, absent 5/8 (v2).

## 5. Decision
Publish the level with its interval and the self-run label. Do not publish
"improved over v1": the interval includes zero. Grow the absent stratum
before the next claim; 8 items cannot support one.

## 6. Caveats
Self-run benchmark on a private set; toy-scale n; absent stratum
under-powered; single labeler pair at toy scale.

## 7. Tiers
All run numbers measured on the shipped fixture. Inter-annotator agreement
illustrative at this scale, measured at real scale.

## 9. Next
Retire or refresh if the stratum saturates or leaks; the next public delta
claim needs a set sized for the MDE it wants to detect.
```

## Output of this file

A public-claim pipeline end to end: a scorecard rejection of two public
candidates including a stale one, a stratified labeled fixture with negative
space and a plant, a fingerprint refusal demonstrated live, every number
carrying an interval, a delta claim declined because the interval includes
zero, and a report a hostile reader cannot catch flattering itself.
