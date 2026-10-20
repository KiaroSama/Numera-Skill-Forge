#!/usr/bin/env python3
"""Paired bootstrap interval on per-item deltas from two harness-agnostic result files.

Input: JSON {"items":[...]}, a JSON array, or JSONL. Records need a unique, non-empty
string ``id`` and a finite numeric ``score``. Optional ``cluster`` values must be JSON
scalars and must match between arms. The resample needs at least 4 independent units
(distinct ``cluster`` values, or items when unclustered) and at least 200 reps.
Exit 0 on valid input and 1 on invalid input.
"""
import argparse
import json
import math
import random
import statistics
import sys

# With U units drawn U times per rep, "every draw hits one unit" carries U^(1-U)
# of the resample mass: 50% at U=2, 11% at U=3, 1.6% at U=4. Below 4 units the
# 2.5% tails land inside the extreme atoms, so the interval is pinned near
# [min, max] of the unit means and "excludes zero" just restates that they
# share a sign.
MIN_UNITS = 4
# Below ~10 units the resample distribution is still visibly quantized; the
# interval is computable but coarse, so the output carries a caution.
WARN_UNITS = 10
# A 95% nearest-rank interval needs enough reps for the 2.5%/97.5% tails to be
# meaningful order statistics; at reps <= 40 the lower bound is the resample
# minimum outright, and at reps 1 the interval is a single point that always
# "excludes zero" for any nonzero mean.
MIN_REPS = 200
MAX_REPS = 100_000
MAX_RESAMPLED_UNITS = 10_000_000


def fail(message):
    print(f"error: {message}", file=sys.stderr)
    sys.exit(1)


def _reject_dup_keys(pairs):
    obj = {}
    for key, value in pairs:
        if key in obj:
            raise ValueError(f"duplicate key {key!r}")
        obj[key] = value
    return obj


def parse_records(text):
    try:
        data = json.loads(text, object_pairs_hook=_reject_dup_keys)
        if isinstance(data, dict):
            if "items" not in data:
                raise TypeError("JSON object needs an 'items' array")
            return data["items"]
        return data
    except (json.JSONDecodeError, ValueError, RecursionError):
        return [json.loads(line, object_pairs_hook=_reject_dup_keys)
                for line in text.splitlines() if line.strip()]


def scalar(value):
    return isinstance(value, (str, int, float)) and not isinstance(value, bool) and (not isinstance(value, float) or math.isfinite(value))


def load(path):
    try:
        with open(path, encoding="utf-8-sig") as handle:
            text = handle.read().strip()
    except FileNotFoundError:
        fail(f"file not found: {path}")
    except (OSError, UnicodeError) as exc:
        fail(f"cannot read {path} as UTF-8 ({exc})")
    try:
        items = parse_records(text)
    except (json.JSONDecodeError, ValueError, KeyError, TypeError, RecursionError) as exc:
        fail(f"{path} is not a per-item JSON/JSONL file ({exc}). Need records with 'id' and finite numeric 'score'.")
    if not isinstance(items, list):
        fail(f"{path} items must be an array.")
    result = {}
    for item in items:
        if not isinstance(item, dict):
            fail(f"record {item!r} in {path} must be an object.")
        item_id, score = item.get("id"), item.get("score")
        if not isinstance(item_id, str) or not item_id.strip():
            fail(f"record {item!r} in {path} needs a non-empty string 'id'.")
        try:
            finite_score = isinstance(score, (int, float)) and not isinstance(score, bool) and math.isfinite(score)
        except OverflowError:
            finite_score = False
        if not finite_score:
            fail(f"record {item!r} in {path} needs a finite numeric 'score'.")
        cluster = item.get("cluster", item_id)
        if not scalar(cluster):
            fail(f"record {item!r} in {path} needs a scalar string or number 'cluster'.")
        if item_id in result:
            fail(f"{path} contains duplicate item id {item_id!r}.")
        result[item_id] = item
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--a", required=True, help="baseline per-item file")
    parser.add_argument("--b", required=True, help="treatment per-item file")
    parser.add_argument("--reps", type=int, default=5000,
                        help=f"bootstrap repetitions ({MIN_REPS}-{MAX_REPS})")
    parser.add_argument("--seed", type=int, default=0)
    args = parser.parse_args()
    if not MIN_REPS <= args.reps <= MAX_REPS:
        fail(f"--reps must be between {MIN_REPS} and {MAX_REPS}.")

    a_items, b_items = load(args.a), load(args.b)
    if set(a_items) != set(b_items):
        fail(f"item ids differ ({len(set(a_items)-set(b_items))} only in a, {len(set(b_items)-set(a_items))} only in b). Paired analysis needs identical item sets.")
    ids = sorted(a_items)
    mismatches = [item_id for item_id in ids
                  if a_items[item_id].get("cluster", item_id) != b_items[item_id].get("cluster", item_id)]
    if mismatches:
        fail("cluster assignments differ between arms for ids: " + ", ".join(mismatches[:20]))

    deltas = {}
    for item_id in ids:
        # int scores can exceed float range: the subtraction stays exact in Python,
        # so coerce here — float() raises OverflowError, and from this point every
        # arithmetic overflow downstream yields inf, which the isfinite guards catch.
        try:
            delta = float(b_items[item_id]["score"] - a_items[item_id]["score"])
        except OverflowError:
            delta = math.inf
        if not math.isfinite(delta):
            fail(f"score delta for item {item_id!r} is not finite (values overflow or are too large).")
        deltas[item_id] = delta
    clusters = {}
    for item_id in ids:
        clusters.setdefault(a_items[item_id].get("cluster", item_id), []).append(deltas[item_id])
    units = list(clusters.values())
    if len(units) < MIN_UNITS:
        if len(units) < len(ids):
            fail(f"only {len(units)} independent resampling units ({len(ids)} items in "
                 f"{len(units)} cluster(s)): a cluster bootstrap needs at least {MIN_UNITS} "
                 "units or the interval tails collapse onto extreme draws. Score items "
                 "from more distinct parents/clusters; omit 'cluster' only when items "
                 "are genuinely independent.")
        fail(f"need at least {MIN_UNITS} paired items to compute a bootstrap interval.")
    if len(ids) * args.reps > MAX_RESAMPLED_UNITS:
        fail(f"requested bootstrap work is too large: {len(ids)} items x {args.reps} reps exceeds {MAX_RESAMPLED_UNITS} resampled item values. Reduce --reps or the evaluation set size.")
    unit_name = "clusters" if len(units) < len(ids) else "items"
    if len(units) < WARN_UNITS:
        print(f"caution: only {len(units)} resampling {unit_name}; the interval is "
              "coarse, treat the verdict as weak evidence.", file=sys.stderr)
    rng = random.Random(args.seed)
    means = []
    for _ in range(args.reps):
        flat = [delta for _unit in units for delta in rng.choice(units)]
        sample_mean = sum(flat) / len(flat)
        if not math.isfinite(sample_mean):
            fail("bootstrap sample overflowed; scores are too large.")
        means.append(sample_mean)
    means.sort()
    # Nearest-rank percentile: the 1-indexed rank ceil(p*reps) maps to the
    # 0-indexed value ceil(p*reps)-1. Using int() truncated toward the middle,
    # narrowing the interval (the anti-conservative direction) whenever p*reps
    # landed on an integer or fractional boundary.
    low = means[max(0, math.ceil(0.025 * args.reps) - 1)]
    high = means[min(args.reps - 1, max(0, math.ceil(0.975 * args.reps) - 1))]
    mean = statistics.fmean(deltas.values())
    if not math.isfinite(mean):
        fail("mean delta overflowed; scores are too large.")
    improved = sum(delta > 0 for delta in deltas.values())
    regressed = sum(delta < 0 for delta in deltas.values())
    if low == high:
        print("caution: the interval is a single point (resampled deltas show no "
              "variation); a zero-width interval cannot support a significance claim.",
              file=sys.stderr)
    print(f"n {len(ids)} items, resampled over {len(units)} {unit_name}, {args.reps} reps, seed {args.seed}")
    print(f"mean delta (b - a) {mean:+.4f}   95% interval [{low:+.4f}, {high:+.4f}]")
    print(f"improved {improved}  regressed {regressed}  held {len(ids)-improved-regressed}")
    print("interval includes zero: not distinguishable from noise on this fixture" if low <= 0 <= high else "interval excludes zero on this fixture (still subject to fixture validity and label error)")


if __name__ == "__main__":
    main()
