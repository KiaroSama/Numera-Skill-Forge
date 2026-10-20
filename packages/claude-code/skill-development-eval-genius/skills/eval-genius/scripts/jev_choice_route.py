#!/usr/bin/env python3
"""Route judged-lane choice/score items by confidence or top-two margin.

The decision-model judge also answers multiple-choice and rubric-score items,
not just yes/no. This script routes those answers into the same three buckets
as jev_confidence_route.py, but it is a SEPARATE tool on purpose: thresholds
that work for a single yes/no `noul` do not transfer to choice or score items.
The routing value here is either the item's `confidence` scalar (--by
confidence, the default) or the margin between the top two labels in
`probabilities` (--by margin).

  value > --high   auto-accept   trust the answer
  value < --low    auto-reject   the judge is sure it is wrong
  otherwise        escalate      a human, or a slower lane, decides

Both boundaries escalate on purpose: an exactly-at-threshold item is not
evidence of confidence either way.

Input: JSONL, one object per line, on stdin or via --input FILE:
  {"id": <any JSON value>,
   "type": "choice" | "score",
   "confidence": <number in [0,1]>,
   "probabilities": {<label>: <number in [0,1]>},
   "answer": <chosen label or score value, optional>}

`probabilities` must be non-empty and its values must sum to 1 within ±0.02.
Under --by margin the routing value is top1 - top2 of `probabilities`; an item
with a single label routes on that label's probability alone. When `answer`
is absent or null the reported answer is the argmax label.

Output: one "<id>\\t<answer>\\t<route>" line per item, then a summary line with
the count in each bucket; --json emits {"routes": [...], "summary": {...}}.

The thresholds are yours, chosen from your own calibration
(references/15-decision-model-judge.md); the defaults below are illustrative,
not recommended values, and they do NOT transfer from the noul router.

Usage:
  jev_choice_route.py --by confidence --low 0.3 --high 0.7 < answers.jsonl
  jev_choice_route.py --by margin --input answers.jsonl --json
Exit 0 on success, 1 on malformed input or bad thresholds.
"""
import argparse
import json
import math
import sys

if hasattr(sys.stdout, "reconfigure"):
    sys.stdout.reconfigure(encoding="utf-8", errors="backslashreplace")

ROUTES = ("auto-accept", "auto-reject", "escalate")
TYPES = ("choice", "score")
SUM_TOLERANCE = 0.02


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


def read_lines(input_path):
    if input_path is None:
        return sys.stdin.read().splitlines()
    try:
        with open(input_path, encoding="utf-8-sig") as handle:
            return handle.read().splitlines()
    except FileNotFoundError:
        fail(f"input file not found: {input_path}")
    except (OSError, UnicodeError) as exc:
        fail(f"cannot read {input_path} as UTF-8 ({exc})")


def _check_tab_free(value, lineno, what):
    if isinstance(value, str) and any(ch in value for ch in "\t\r\n"):
        fail(f"line {lineno}: {what} {value!r} contains a tab or newline and "
             "cannot be printed in the per-item output; re-encode it upstream.")


def _is_number(value):
    return not isinstance(value, bool) and isinstance(value, (int, float))


def parse_records(lines):
    records = []
    for lineno, line in enumerate(lines, start=1):
        if not line.strip():
            continue
        try:
            record = json.loads(line, object_pairs_hook=_reject_dup_keys)
        except (json.JSONDecodeError, ValueError, RecursionError) as exc:
            fail(f"line {lineno}: malformed JSON ({exc})")
        if not isinstance(record, dict):
            fail(f"line {lineno}: each record must be a JSON object, got {record!r}")
        if "id" not in record:
            fail(f"line {lineno}: record is missing 'id'.")
        _check_tab_free(record["id"], lineno, "item id")

        if "type" not in record:
            fail(f"line {lineno}: record {record['id']!r} is missing 'type'.")
        if record["type"] not in TYPES:
            fail(f"line {lineno}: type for {record['id']!r} must be one of "
                 f"{TYPES}, got {record['type']!r}")

        if "confidence" not in record:
            fail(f"line {lineno}: record {record['id']!r} is missing 'confidence'.")
        confidence = record["confidence"]
        if not _is_number(confidence):
            fail(f"line {lineno}: confidence for {record['id']!r} must be a "
                 f"number, got {confidence!r}")
        if not math.isfinite(confidence) or not 0.0 <= confidence <= 1.0:
            fail(f"line {lineno}: confidence for {record['id']!r} must be in "
                 f"[0,1], got {confidence!r}")

        if "probabilities" not in record:
            fail(f"line {lineno}: record {record['id']!r} is missing 'probabilities'.")
        probabilities = record["probabilities"]
        if not isinstance(probabilities, dict) or not probabilities:
            fail(f"line {lineno}: probabilities for {record['id']!r} must be a "
                 f"non-empty object, got {probabilities!r}")
        total = 0.0
        for label, probability in probabilities.items():
            if not _is_number(probability):
                fail(f"line {lineno}: probability for label {label!r} of "
                     f"{record['id']!r} must be a number, got {probability!r}")
            if not math.isfinite(probability) or not 0.0 <= probability <= 1.0:
                fail(f"line {lineno}: probability for label {label!r} of "
                     f"{record['id']!r} must be in [0,1], got {probability!r}")
            total += probability
        if abs(total - 1.0) > SUM_TOLERANCE:
            fail(f"line {lineno}: probabilities for {record['id']!r} sum to "
                 f"{total:g}, not 1 within +/-{SUM_TOLERANCE:g}.")

        answer = record.get("answer")
        if answer is not None:
            _check_tab_free(answer, lineno, "answer")
        records.append(record)
    return records


def argmax_label(probabilities):
    best_label, best_probability = None, -1.0
    for label, probability in probabilities.items():
        if probability > best_probability:
            best_label, best_probability = label, probability
    return best_label


def routing_value(record, by):
    if by == "confidence":
        return record["confidence"]
    ordered = sorted(record["probabilities"].values(), reverse=True)
    runner_up = ordered[1] if len(ordered) > 1 else 0.0
    return ordered[0] - runner_up


def route(value, low, high):
    if value > high:
        return "auto-accept"
    if value < low:
        return "auto-reject"
    return "escalate"


def format_value(value):
    if isinstance(value, str):
        return value
    return json.dumps(value, ensure_ascii=False)


def main():
    parser = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--input", default=None,
                        help="JSONL file of choice/score records (default: stdin)")
    parser.add_argument("--by", choices=("confidence", "margin"), default="confidence",
                        help="route on the confidence scalar or the top-two probability "
                             "margin (default: confidence)")
    parser.add_argument("--low", type=float, default=0.3,
                        help="auto-reject boundary: value strictly below this (default: 0.3, illustrative)")
    parser.add_argument("--high", type=float, default=0.7,
                        help="auto-accept boundary: value strictly above this (default: 0.7, illustrative)")
    parser.add_argument("--json", action="store_true",
                        help="emit a JSON object instead of per-item lines")
    args = parser.parse_args()

    for name, value in (("--low", args.low), ("--high", args.high)):
        if not math.isfinite(value) or not 0.0 <= value <= 1.0:
            fail(f"{name} must be a finite number in [0,1].")
    if args.low > args.high:
        fail(f"--low ({args.low}) must not exceed --high ({args.high}).")

    records = parse_records(read_lines(args.input))
    routes = []
    for record in records:
        answer = record["answer"] if record.get("answer") is not None \
            else argmax_label(record["probabilities"])
        value = routing_value(record, args.by)
        routes.append({"id": record["id"], "answer": answer, "value": value,
                       "route": route(value, args.low, args.high)})
    summary = {name: sum(1 for entry in routes if entry["route"] == name)
               for name in ROUTES}

    if args.json:
        print(json.dumps({
            "routes": routes,
            "summary": summary,
            "by": args.by,
            "thresholds": {"low": args.low, "high": args.high},
        }, ensure_ascii=False))
        return

    for entry in routes:
        print(f"{format_value(entry['id'])}\t{format_value(entry['answer'])}\t{entry['route']}")
    print("summary  " + "  ".join(f"{name} {summary[name]}" for name in ROUTES)
          + f"  total {len(routes)}")


if __name__ == "__main__":
    main()
