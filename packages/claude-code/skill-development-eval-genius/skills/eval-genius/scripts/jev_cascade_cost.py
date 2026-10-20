#!/usr/bin/env python3
"""Projected cost of the judged lane in a two-tier cascade.

A cheap deterministic filter runs on every item; only the residue that trips it
reaches a decision-model judge billed per input token. The bill is linear in the
residue rate: judged_items = round(items * residue_rate), judged_tokens =
judged_items * tokens_per_item, cost = judged_tokens / 1e9 * price_per_btok.
Items resolved by the filter cost nothing here — this is the marginal spend of
turning the judged lane on. Measure the residue rate and tokens per item on a
real slice of your own traffic; neither is knowable from the defaults.

Two optional refinements, each off unless its flags are passed:

* Batching. --state-tokens S --question-tokens T --questions-per-item Q models
  one judged request that carries Q questions against a shared state prefix:
  unbatched_tokens_per_item = Q*(S+T), batched_tokens_per_item = S + Q*T. The
  batched figure is the billed request size, and the script warns on stderr
  when it exceeds the 64000-token request budget. S and T must be given
  together; when both are absent the flat --tokens-per-item model applies
  exactly as before (pass one model or the other, not both).
* Escalation tier. --escalation-rate R --escalation-cost-per-item C prices the
  share of judged items that bounce to a human or slower lane:
  escalation_cost = judged_items * R * C, printed separately and added into
  total_cascade_cost = judge_cost + escalation_cost. Off by default (R = 0).

Usage:
  jev_cascade_cost.py --items 100000 --residue-rate 0.05 --tokens-per-item 2000
  jev_cascade_cost.py --items 100000 --residue-rate 0.05 --state-tokens 3000 --question-tokens 1500 --questions-per-item 4
  jev_cascade_cost.py --items 100000 --residue-rate 0.05 --tokens-per-item 2000 --escalation-rate 0.1 --escalation-cost-per-item 0.5 --json
Exit 0 on success, 1 on invalid input.
"""
import argparse
import json
import math
import sys


def fail(message):
    print(f"error: {message}", file=sys.stderr)
    sys.exit(1)


def checked_finite(value, name):
    if not math.isfinite(value):
        fail(f"{name} must be a finite number.")
    return value


def whole_number(value, name):
    checked_finite(value, name)
    if not value.is_integer():
        fail(f"{name} is an item count and must be a whole number.")
    return int(value)


def main():
    parser = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--items", type=float, required=True,
                        help="total items entering the cascade per period (whole number, >0)")
    parser.add_argument("--residue-rate", type=float, required=True,
                        help="fraction of items the deterministic filter escalates to the judge, in [0,1]")
    parser.add_argument("--tokens-per-item", type=float, default=None,
                        help="average judge input tokens per judged item (whole number, >0; "
                             "required unless --state-tokens and --question-tokens are given)")
    parser.add_argument("--price-per-btok", type=float, default=42.0,
                        help="dollars per billion judge input tokens (default: 42.0)")
    parser.add_argument("--questions-per-item", type=float, default=1.0,
                        help="questions bundled into one judged item (whole number, >=1; default: 1)")
    parser.add_argument("--state-tokens", type=float, default=None,
                        help="shared state-prefix tokens per judged item (whole number, >=0; "
                             "requires --question-tokens)")
    parser.add_argument("--question-tokens", type=float, default=None,
                        help="tokens per question inside a judged item (whole number, >=0; "
                             "requires --state-tokens)")
    parser.add_argument("--escalation-rate", type=float, default=0.0,
                        help="fraction of judged items escalated to the slower tier, in [0,1] "
                             "(default: 0, off)")
    parser.add_argument("--escalation-cost-per-item", type=float, default=0.0,
                        help="dollars per escalated item (>=0, default: 0.0)")
    parser.add_argument("--json", action="store_true",
                        help="emit a JSON object instead of human-readable lines")
    args = parser.parse_args()

    items = whole_number(args.items, "--items")
    if items <= 0:
        fail("--items must be greater than zero.")

    questions = whole_number(args.questions_per_item, "--questions-per-item")
    if questions < 1:
        fail("--questions-per-item must be at least 1.")
    state_given = args.state_tokens is not None
    if state_given != (args.question_tokens is not None):
        fail("--state-tokens and --question-tokens must be given together.")
    unbatched_tokens_per_item = batched_tokens_per_item = None
    if state_given:
        if args.tokens_per_item is not None:
            fail("use either --tokens-per-item or --state-tokens/--question-tokens, not both.")
        state_tokens = whole_number(args.state_tokens, "--state-tokens")
        question_tokens = whole_number(args.question_tokens, "--question-tokens")
        if state_tokens < 0 or question_tokens < 0:
            fail("--state-tokens and --question-tokens must not be negative.")
        unbatched_tokens_per_item = questions * (state_tokens + question_tokens)
        batched_tokens_per_item = state_tokens + questions * question_tokens
        if batched_tokens_per_item < 1:
            fail("--state-tokens and --question-tokens must total at least 1 token per item.")
        tokens_per_item = batched_tokens_per_item
    else:
        if args.tokens_per_item is None:
            parser.error("--tokens-per-item is required unless --state-tokens and "
                         "--question-tokens are given.")
        tokens_per_item = whole_number(args.tokens_per_item, "--tokens-per-item")
        if tokens_per_item <= 0:
            fail("--tokens-per-item must be greater than zero.")

    residue_rate = checked_finite(args.residue_rate, "--residue-rate")
    if not 0.0 <= residue_rate <= 1.0:
        fail("--residue-rate must be between 0 and 1, inclusive.")
    price = checked_finite(args.price_per_btok, "--price-per-btok")
    if price < 0.0:
        fail("--price-per-btok must not be negative.")
    escalation_rate = checked_finite(args.escalation_rate, "--escalation-rate")
    if not 0.0 <= escalation_rate <= 1.0:
        fail("--escalation-rate must be between 0 and 1, inclusive.")
    escalation_cost_per_item = checked_finite(args.escalation_cost_per_item,
                                              "--escalation-cost-per-item")
    if escalation_cost_per_item < 0.0:
        fail("--escalation-cost-per-item must not be negative.")

    judged_items = round(items * residue_rate)
    judged_tokens = judged_items * tokens_per_item
    cost = judged_tokens / 1e9 * price
    if not math.isfinite(cost):
        fail("projected cost overflowed; the inputs are too large to price meaningfully.")
    cost = round(cost, 4)

    if batched_tokens_per_item is not None and batched_tokens_per_item > 64000:
        print(f"warning: batched request is {batched_tokens_per_item} tokens per "
              "item, over the 64000-token request budget; split the batch or "
              "trim the state prefix.", file=sys.stderr)

    escalation_active = escalation_rate > 0.0 or escalation_cost_per_item > 0.0
    escalation_cost = total_cost = 0.0
    if escalation_active:
        escalation_cost = judged_items * escalation_rate * escalation_cost_per_item
        if not math.isfinite(escalation_cost):
            fail("escalation cost overflowed; the inputs are too large to price meaningfully.")
        escalation_cost = round(escalation_cost, 4)
        total_cost = round(cost + escalation_cost, 4)

    if residue_rate == 0.0:
        note = ("residue rate is 0: nothing reaches the judged lane. If that surprises "
                "you, the deterministic filter is absorbing everything; re-measure the "
                "rate before trusting the $0 projection.")
    elif residue_rate == 1.0:
        note = ("residue rate is 1: every item reaches the judge, so the cascade saves "
                "nothing over judging directly; the filter exists only to add latency.")
    else:
        note = (f"{residue_rate:.1%} of items escalate to the judged lane. Cost scales "
                "linearly with residue rate and tokens per item; tighten the cheap "
                "filter or cap judged tokens to cut spend.")

    if args.json:
        payload = {
            "items": items,
            "residue_rate": residue_rate,
            "tokens_per_item": tokens_per_item,
            "price_per_btok": price,
            "judged_items": judged_items,
            "judged_tokens": judged_tokens,
            "projected_cost_usd": cost,
        }
        if batched_tokens_per_item is not None:
            payload.update({
                "questions_per_item": questions,
                "state_tokens": state_tokens,
                "question_tokens": question_tokens,
                "unbatched_tokens_per_item": unbatched_tokens_per_item,
                "batched_tokens_per_item": batched_tokens_per_item,
                "token_savings_ratio": round(unbatched_tokens_per_item
                                           / batched_tokens_per_item, 4),
            })
        if escalation_active:
            payload.update({
                "escalation_rate": escalation_rate,
                "escalation_cost_per_item": escalation_cost_per_item,
                "escalation_cost_usd": escalation_cost,
                "total_cascade_cost_usd": total_cost,
            })
        payload["escalation"] = note
        print(json.dumps(payload, ensure_ascii=False))
        return

    print(f"items {items}  residue rate {residue_rate:.4f}  judged items {judged_items}")
    if batched_tokens_per_item is not None:
        print(f"unbatched tokens per item {unbatched_tokens_per_item}  "
              f"batched tokens per item {batched_tokens_per_item}  "
              f"savings {unbatched_tokens_per_item / batched_tokens_per_item:.2f}x")
    print(f"tokens per item {tokens_per_item}  judged tokens {judged_tokens}  "
          f"price ${price:.2f} per Btok")
    print(f"projected cost ${cost:.4f}")
    if escalation_active:
        print(f"escalation rate {escalation_rate:.4f}  cost per item "
              f"${escalation_cost_per_item:.4f}  escalation cost ${escalation_cost:.4f}")
        print(f"total cascade cost ${total_cost:.4f}")
    print(f"note: {note}")


if __name__ == "__main__":
    main()
