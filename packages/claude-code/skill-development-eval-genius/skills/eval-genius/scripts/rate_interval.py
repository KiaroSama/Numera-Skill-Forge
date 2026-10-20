#!/usr/bin/env python3
"""Wilson score interval for a single rate: pass rate, activation rate, judge
positive rate.

A proportion k out of n without an interval is not a result. The Wilson
interval is the right default for one: it stays inside [0, 1] and stays honest
at the edges (k = 0, k = n, small n) where the normal approximation goes
negative or above 1. For a *delta* between two arms on the same items use
paired_bootstrap.py instead; this script measures one rate on one run.

Usage:
  rate_interval.py --k 31 --n 40                  # 95% interval
  rate_interval.py --k 0 --n 10 --confidence 0.99
Exit 0 on success, 1 on invalid input.
"""
import argparse
import math
import statistics
import sys


def fail(message):
    print(f"error: {message}", file=sys.stderr)
    sys.exit(1)


def checked_finite(value, name):
    if not math.isfinite(value):
        fail(f"{name} must be a finite number.")
    return value


def wilson(k, n, confidence):
    z = statistics.NormalDist().inv_cdf((1.0 + confidence) / 2.0)
    z2 = z * z
    point = k / n
    denom = 1.0 + z2 / n
    center = (point + z2 / (2.0 * n)) / denom
    half = (z / denom) * math.sqrt(point * (1.0 - point) / n + z2 / (4.0 * n * n))
    return point, max(0.0, center - half), min(1.0, center + half)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--k", type=float, required=True,
                        help="successes: items that passed, fired, or agreed (whole number)")
    parser.add_argument("--n", type=float, required=True,
                        help="trials: total items scored (whole number)")
    parser.add_argument("--confidence", type=float, default=0.95,
                        help="two-sided confidence level (default: 0.95)")
    args = parser.parse_args()

    k = checked_finite(args.k, "--k")
    n = checked_finite(args.n, "--n")
    confidence = checked_finite(args.confidence, "--confidence")
    if n <= 0:
        fail("--n must be greater than zero.")
    if not 0.0 <= k <= n:
        fail("--k must satisfy 0 <= k <= n.")
    if not k.is_integer() or not n.is_integer():
        fail("--k and --n are item counts and must be whole numbers.")
    if k == 0:
        k = 0.0  # normalize -0.0 so it does not print as "-0"
    if not 0.0 < confidence < 1.0:
        fail("--confidence must be between 0 and 1, exclusive.")
    # A confidence so close to 1 that (1+confidence)/2 rounds to 1.0 has no finite z.
    if (1.0 + confidence) / 2.0 >= 1.0:
        fail("--confidence is too close to 1 to compute a finite interval.")

    point, low, high = wilson(k, n, confidence)
    if f"{low:.4f}" == f"{high:.4f}":
        print("caution: the interval is a single point at printed precision; a "
              "zero-width interval cannot support a significance claim.",
              file=sys.stderr)
    print(f"k {k:g}  n {n:g}  point estimate {point:.4f}")
    print(f"{confidence * 100:g}% Wilson interval [{low:.4f}, {high:.4f}]")


if __name__ == "__main__":
    main()
