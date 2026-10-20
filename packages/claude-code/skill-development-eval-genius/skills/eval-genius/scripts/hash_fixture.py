#!/usr/bin/env python3
"""Canonical content hash of a frozen fixture, for the manifest's fixture_hash.

Every gate/bootstrap run is refused without a fixture_hash, and two runs are only
comparable when their hashes match. This produces that hash the same way every time,
so two people (or two tools) hashing the same fixture get the same string instead of
each inventing a canonicalization and silently failing to compare.

Canonicalization: parse the fixture, re-serialize with sorted keys and no incidental
whitespace, UTF-8. Formatting, key order, record order (including same-id records),
and trailing newlines do not change the hash; content does. Accepts a JSON object
with an 'items' array (sibling keys like 'version' are included in the hash, so
version:1 and version:2 produce different hashes), a bare array, or JSONL (one
record per line). A bare array, JSONL, and an object whose only key is 'items' all
hash to the same value when they carry the same records. Empty fixtures and
duplicate object keys are refused — a hash that ignores content is worse than no hash.

What is hashed: the full canonical object. For a bare array or JSONL, that is the
sorted list of records. For an object with only 'items', that is the sorted items
array (identical to the bare-array hash). For an object with 'items' plus sibling
keys, that is the full object with its items array sorted, so every top-level key
contributes to the hash.

Usage:
  hash_fixture.py path/to/fixture.json         # prints: sha256:<hex>
  hash_fixture.py --raw path/to/fixture.json   # prints just <hex>
Exit 0 on success, 2 on unreadable or non-JSON input.
"""
import argparse
import hashlib
import json
import sys


def die(msg):
    print(f"error: {msg}", file=sys.stderr)
    sys.exit(2)


def _reject_dup_keys(pairs):
    obj = {}
    for key, value in pairs:
        if key in obj:
            raise ValueError(f"duplicate key {key!r}")
        obj[key] = value
    return obj


def load_records(path):
    try:
        with open(path, encoding="utf-8-sig") as handle:
            text = handle.read().strip()
    except FileNotFoundError:
        die(f"file not found: {path}")
    except (OSError, UnicodeError) as exc:
        die(f"cannot read {path} as UTF-8 ({exc})")
    if not text:
        die(f"{path} is empty.")
    # json.loads accepts duplicate object keys last-wins; that collapses distinct
    # content to one hash, so refuse them via object_pairs_hook. JSONDecodeError is
    # a ValueError subclass and must stay the first except (it triggers the JSONL
    # fallback); a bare ValueError here is the hook or the int parser limit.
    try:
        data = json.loads(text, object_pairs_hook=_reject_dup_keys)
    except json.JSONDecodeError:
        try:
            return [json.loads(line, object_pairs_hook=_reject_dup_keys)
                    for line in text.splitlines() if line.strip()]
        except json.JSONDecodeError as exc:
            die(f"{path} is neither a JSON document nor JSONL ({exc}).")
        except (ValueError, RecursionError) as exc:
            die(f"{path} contains duplicate object keys, an out-of-range value, or is too deeply nested ({exc}).")
    except (ValueError, RecursionError) as exc:
        die(f"{path} contains duplicate object keys, an out-of-range value, or is too deeply nested ({exc}).")
    if isinstance(data, dict):
        if "items" not in data:
            return data  # canonical_hash refuses it: not a list
        extra = sorted(set(data) - {"items"})
        if extra:
            return data  # full object with sibling keys — hash includes them
        return data["items"]  # only 'items' — hash as a list, same as a bare array
    return data


def _sort_records(records):
    # Sort by id, then by full canonical content, so record order never changes
    # the hash — including same-id records, where input order used to leak through
    # the stable sort. A fixture is a set of items, not a sequence.
    def sort_key(record):
        ident = json.dumps(record.get("id") if isinstance(record, dict) else record,
                           sort_keys=True, ensure_ascii=False)
        return ident, json.dumps(record, sort_keys=True, ensure_ascii=False)
    return sorted(records, key=sort_key)


def canonical_hash(data):
    if isinstance(data, dict):
        items = data.get("items")
        if not isinstance(items, list):
            die("fixture must be a list of records (or an object with an 'items' array).")
        if not items:
            die("fixture contains no records; an empty fixture cannot anchor a run.")
        data = {**data, "items": _sort_records(items)}
    elif isinstance(data, list):
        if not data:
            die("fixture contains no records; an empty fixture cannot anchor a run.")
        data = _sort_records(data)
    else:
        die("fixture must be a list of records (or an object with an 'items' array).")
    blob = json.dumps(data, sort_keys=True, separators=(",", ":"), ensure_ascii=False)
    return hashlib.sha256(blob.encode("utf-8")).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("fixture", help="path to the fixture (JSON or JSONL)")
    parser.add_argument("--raw", action="store_true", help="print the bare hex, no 'sha256:' prefix")
    args = parser.parse_args()
    digest = canonical_hash(load_records(args.fixture))
    print(digest if args.raw else f"sha256:{digest}")


if __name__ == "__main__":
    main()
