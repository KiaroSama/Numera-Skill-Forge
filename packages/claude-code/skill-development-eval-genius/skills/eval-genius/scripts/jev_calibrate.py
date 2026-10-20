#!/usr/bin/env python3
"""Fit a temperature-scaling calibration transform on your own scored items.

A judge or model emits a `raw` confidence in [0,1]; you also hold the true
`label` (0 or 1). Temperature scaling divides the logit by T > 0:

    p_calibrated = sigmoid(logit(raw) / T)

T = 1 leaves the probability alone, T > 1 softens toward 0.5, T < 1 sharpens.
This script finds the T that minimizes negative log-likelihood on YOUR data
with a golden-section search over log T — there is no baked-in constant — then
reports the fixed 10-bin expected calibration error (ECE) before and after.

The fitted T is fit from YOUR data only. Re-fit it whenever the model, prompt,
or traffic distribution drifts: a temperature measured on last month's judge
says nothing about this month's.

Input: JSONL, one object per line, on stdin or via --input FILE:
  {"id": <any JSON value>, "raw": <number in [0,1]>, "label": 0 or 1}
At least 50 items are required; fewer than that cannot support a fit.

Usage:
  jev_calibrate.py < calibration.jsonl
  jev_calibrate.py --input calibration.jsonl --json
Exit 0 on success, 1 on malformed input, 2 CANNOT-MEASURE (fewer than 50
items, a label outside {0,1}, or a raw outside [0,1]).
"""
import argparse
import json
import math
import sys

if hasattr(sys.stdout, "reconfigure"):
    sys.stdout.reconfigure(encoding="utf-8", errors="backslashreplace")

MIN_ITEMS = 50
ECE_BINS = 10
EPS = 1e-9
LOG_T_LO, LOG_T_HI = math.log(1e-4), math.log(1e4)
NOTE = ("this transform is fit from YOUR data only; re-fit it whenever the "
        "model, prompt, or traffic distribution drifts before trusting it.")


def fail(message, code=1):
    print(f"error: {message}", file=sys.stderr)
    sys.exit(code)


def cannot_measure(message):
    fail(f"CANNOT-MEASURE: {message}", code=2)


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
        if "raw" not in record:
            fail(f"line {lineno}: record {record['id']!r} is missing 'raw'.")
        if "label" not in record:
            fail(f"line {lineno}: record {record['id']!r} is missing 'label'.")
        raw = record["raw"]
        if not _is_number(raw):
            fail(f"line {lineno}: raw for {record['id']!r} must be a number, "
                 f"got {raw!r}")
        if not math.isfinite(raw) or not 0.0 <= raw <= 1.0:
            cannot_measure(f"line {lineno}: raw for {record['id']!r} must be in "
                           f"[0,1], got {raw!r}")
        label = record["label"]
        if not _is_number(label):
            fail(f"line {lineno}: label for {record['id']!r} must be 0 or 1, "
                 f"got {label!r}")
        if label not in (0, 1):
            cannot_measure(f"line {lineno}: label for {record['id']!r} must be "
                           f"0 or 1, got {label!r}")
        records.append({"id": record["id"], "raw": float(raw),
                        "label": int(label)})
    if len(records) < MIN_ITEMS:
        cannot_measure(f"only {len(records)} usable items; at least {MIN_ITEMS} "
                       "are needed to fit a calibration transform.")
    return records


def _clamp(p):
    return min(max(p, EPS), 1.0 - EPS)


def _logit(p):
    p = _clamp(p)
    return math.log(p / (1.0 - p))


def _sigmoid(x):
    if x >= 0.0:
        return 1.0 / (1.0 + math.exp(-x))
    e = math.exp(x)
    return e / (1.0 + e)


def calibrated_probability(logit, temperature):
    return _sigmoid(logit / temperature)


def negative_log_likelihood(log_temperature, logits, labels):
    temperature = math.exp(log_temperature)
    total = 0.0
    for logit, label in zip(logits, labels):
        p = _clamp(calibrated_probability(logit, temperature))
        total -= label * math.log(p) + (1 - label) * math.log(1.0 - p)
    return total


def fit_temperature(logits, labels):
    # Golden-section search on log T: the likelihood is smooth and unimodal
    # over this bracket for any sane dataset, and log-space keeps T > 0.
    invphi = (math.sqrt(5.0) - 1.0) / 2.0
    a, b = LOG_T_LO, LOG_T_HI
    c = b - invphi * (b - a)
    d = a + invphi * (b - a)
    fc = negative_log_likelihood(c, logits, labels)
    fd = negative_log_likelihood(d, logits, labels)
    for _ in range(300):
        if b - a < 1e-12:
            break
        if fc < fd:
            b, d, fd = d, c, fc
            c = b - invphi * (b - a)
            fc = negative_log_likelihood(c, logits, labels)
        else:
            a, c, fc = c, d, fd
            d = a + invphi * (b - a)
            fd = negative_log_likelihood(d, logits, labels)
    return math.exp((a + b) / 2.0)


def expected_calibration_error(probabilities, labels, bins=ECE_BINS):
    n = len(probabilities)
    buckets = [[] for _ in range(bins)]
    for probability, label in zip(probabilities, labels):
        buckets[min(int(probability * bins), bins - 1)].append((probability, label))
    error = 0.0
    for bucket in buckets:
        if not bucket:
            continue
        mean_p = sum(p for p, _ in bucket) / len(bucket)
        mean_y = sum(y for _, y in bucket) / len(bucket)
        error += len(bucket) / n * abs(mean_p - mean_y)
    return error


def main():
    parser = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--input", default=None,
                        help="JSONL file of {\"id\", \"raw\", \"label\"} records (default: stdin)")
    parser.add_argument("--json", action="store_true",
                        help="emit a JSON object instead of human-readable lines")
    args = parser.parse_args()

    records = parse_records(read_lines(args.input))
    logits = [_logit(record["raw"]) for record in records]
    labels = [record["label"] for record in records]

    temperature = fit_temperature(logits, labels)
    ece_before = expected_calibration_error(
        [record["raw"] for record in records], labels)
    ece_after = expected_calibration_error(
        [calibrated_probability(logit, temperature) for logit in logits], labels)

    if args.json:
        print(json.dumps({
            "items": len(records),
            "temperature": round(temperature, 6),
            "ece_before": round(ece_before, 6),
            "ece_after": round(ece_after, 6),
            "bins": ECE_BINS,
            "note": NOTE,
        }, ensure_ascii=False))
        return

    print(f"items {len(records)}")
    print(f"temperature {temperature:.4f}")
    print(f"ECE before {ece_before:.4f}  after {ece_after:.4f}  ({ECE_BINS} bins)")
    print(f"note: {NOTE}")


if __name__ == "__main__":
    main()
