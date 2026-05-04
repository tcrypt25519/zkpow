#!/usr/bin/env python3
import json
import re
import sys
from collections import defaultdict
from dataclasses import dataclass


DURATION_RE = re.compile(r"^([0-9]+(?:\.[0-9]+)?)(ns|µs|us|ms|s)$")


def parse_duration_seconds(value):
    match = DURATION_RE.match(str(value))
    if not match:
        return None
    amount = float(match.group(1))
    unit = match.group(2)
    if unit == "ns":
        return amount / 1_000_000_000
    if unit in ("µs", "us"):
        return amount / 1_000_000
    if unit == "ms":
        return amount / 1_000
    if unit == "s":
        return amount
    return None


def span_name(span):
    if not isinstance(span, dict):
        return ""
    name = span.get("name", "")
    if name == "prove" and span.get("mode"):
        return f"{name}[mode={span['mode']}]"
    return name


def record_path(record):
    names = [span_name(span) for span in record.get("spans", [])]
    current = span_name(record.get("span"))
    if current:
        names.append(current)
    return tuple(name for name in names if name)


def compact_path(path):
    names = [name for name in path if name != "runtime.spawn"]
    if names and names[0] == "prove_compressed_detail":
        names = names[1:]
    if not names:
        return "(root)"
    return " / ".join(names)


@dataclass
class Bucket:
    busy: float = 0.0
    idle: float = 0.0
    count: int = 0


def summarize(jsonl_path, output_path, limit=40):
    in_prove = False
    saw_start = False
    saw_finish = False
    buckets = defaultdict(Bucket)
    total_busy = 0.0
    total_idle = 0.0
    close_events = 0

    with open(jsonl_path, "r", encoding="utf-8") as handle:
        for line in handle:
            try:
                record = json.loads(line)
            except json.JSONDecodeError:
                continue

            fields = record.get("fields", {})
            message = fields.get("message")
            if message == "prove_compressed started":
                in_prove = True
                saw_start = True
                continue
            if message and str(message).startswith("prove_compressed finished"):
                in_prove = False
                saw_finish = True
                continue
            if not in_prove or message != "close":
                continue

            busy = parse_duration_seconds(fields.get("time.busy"))
            idle = parse_duration_seconds(fields.get("time.idle"))
            if busy is None and idle is None:
                continue

            path = record_path(record)
            if not path:
                path = (record.get("target", "(unknown)"),)
            label = compact_path(path)
            bucket = buckets[label]
            bucket.busy += busy or 0.0
            bucket.idle += idle or 0.0
            bucket.count += 1
            total_busy += busy or 0.0
            total_idle += idle or 0.0
            close_events += 1

    with open(output_path, "w", encoding="utf-8") as out:
        out.write("prove_compressed internal span summary\n")
        out.write("source: structured tracing close events between prove_compressed start/finish\n")
        out.write("units: wall-clock span time from tracing, not guest cycle counts\n\n")
        out.write(
            "note: tracing spans are nested and may run concurrently; totals are useful for ranking, "
            "not as an additive decomposition of prove_compressed wall time\n\n"
        )
        if not saw_start:
            out.write("No prove_compressed start marker found.\n")
            return
        if close_events == 0:
            out.write(
                "No internal close events found. Rerun with PROVE_COMPRESSED_SPANS=1 "
                "to capture DEBUG-level SP1 spans in logs/run.jsonl.\n"
            )
            return

        out.write(f"close_events={close_events}\n")
        out.write(f"total_busy={total_busy:.6f}s\n")
        out.write(f"total_idle={total_idle:.6f}s\n")
        if not saw_finish:
            out.write("warning=prove_compressed did not finish before the log ended\n")
        out.write("\n")
        out.write(f"{'busy_s':>10} {'idle_s':>10} {'count':>7} span\n")
        out.write(f"{'-' * 10} {'-' * 10} {'-' * 7} {'-' * 4}\n")

        rows = sorted(
            buckets.items(),
            key=lambda item: (-item[1].busy, -item[1].idle, item[0]),
        )
        for label, bucket in rows[:limit]:
            out.write(f"{bucket.busy:10.6f} {bucket.idle:10.6f} {bucket.count:7d} {label}\n")


def main():
    if len(sys.argv) not in (3, 4):
        print(
            "usage: summarize-prove-spans.py <run.jsonl> <output.txt> [limit]",
            file=sys.stderr,
        )
        return 2
    limit = int(sys.argv[3]) if len(sys.argv) == 4 else 40
    summarize(sys.argv[1], sys.argv[2], limit)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
