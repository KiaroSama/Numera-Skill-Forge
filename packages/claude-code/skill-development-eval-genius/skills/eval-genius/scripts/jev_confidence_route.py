#!/usr/bin/env python3
"""Route judged-lane items by the decision-model judge's confidence.

A judge call returns a decision plus a confidence probability `noul` in [0,1].
This script does not judge anything and makes no network calls; it buckets the
confidence you already have into three routes:

  noul > --high   auto-accept   trust the decision
  noul < --low    auto-reject   the judge is sure it is wrong
  otherwise       escalate      a human, or a slower lane, decides

Both boundaries escalate on purpose: an exactly-at-threshold item is not evidence
of confidence either way.

Input: JSONL, one object per line {"id": <any JSON value>, "noul": <number in
[0,1]>}, on stdin or via --input FILE. Output: one "<id>\\t<route>" line per
item, then a summary line with the count in each bucket; --json emits
{"routes": [...], "summary": {...}} instead.

The thresholds are yours, chosen from your own calibration
(references/15-decision-model-judge.md); the defaults below are illustrative,
not recommended values.

Usage:
  jev_confidence_route.py --low 0.3 --high 0.7 < scores.jsonl
  jev_confidence_route.py --input scores.jsonl --json
Exit 0 on success, 1 on malformed input or bad thresholds.
"""
import argparse
import json
import math
import sys

if hasattr(sys.stdout, "reconfigure"):
    sys.stdout.reconfigure(encoding="utf-8", errors="backslashreplace")

ROUTES = ("auto-accept", "auto-reject", "escalate")


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
        if isinstance(record["id"], str) and any(ch in record["id"] for ch in "\t\r\n"):
            fail(f"line {lineno}: item id {record['id']!r} contains a tab or newline "
                 "and cannot be printed in the per-item output; re-encode the id upstream.")
        if "noul" not in record:
            fail(f"line {lineno}: record {record['id']!r} is missing 'noul'.")
        noul = record["noul"]
        if isinstance(noul, bool) or not isinstance(noul, (int, float)):
            fail(f"line {lineno}: noul for {record['id']!r} must be a number, got {noul!r}")
        if not math.isfinite(noul) or not 0.0 <= noul <= 1.0:
            fail(f"line {lineno}: noul for {record['id']!r} must be in [0,1], got {noul!r}")
        records.append(record)
    return records


def route(noul, low, high):
    if noul > high:
        return "auto-accept"
    if noul < low:
        return "auto-reject"
    return "escalate"


def format_id(value):
    if isinstance(value, str):
        return value
    return json.dumps(value, ensure_ascii=False)


def main():
    parser = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--input", default=None,
                        help="JSONL file of {\"id\", \"noul\"} records (default: stdin)")
    parser.add_argument("--low", type=float, default=0.3,
                        help="auto-reject boundary: noul strictly below this (default: 0.3, illustrative)")
    parser.add_argument("--high", type=float, default=0.7,
                        help="auto-accept boundary: noul strictly above this (default: 0.7, illustrative)")
    parser.add_argument("--json", action="store_true",
                        help="emit a JSON object instead of per-item lines")
    args = parser.parse_args()

    for name, value in (("--low", args.low), ("--high", args.high)):
        if not math.isfinite(value) or not 0.0 <= value <= 1.0:
            fail(f"{name} must be a finite number in [0,1].")
    if args.low > args.high:
        fail(f"--low ({args.low}) must not exceed --high ({args.high}).")

    records = parse_records(read_lines(args.input))
    routes = [{"id": record["id"], "noul": record["noul"],
               "route": route(record["noul"], args.low, args.high)}
              for record in records]
    summary = {name: sum(1 for entry in routes if entry["route"] == name)
               for name in ROUTES}

    if args.json:
        print(json.dumps({
            "routes": routes,
            "summary": summary,
            "thresholds": {"low": args.low, "high": args.high},
        }, ensure_ascii=False))
        return

    for entry in routes:
        print(f"{format_id(entry['id'])}\t{entry['route']}")
    print("summary  " + "  ".join(f"{name} {summary[name]}" for name in ROUTES)
          + f"  total {len(routes)}")


if __name__ == "__main__":
    main()
