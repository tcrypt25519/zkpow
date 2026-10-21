#!/usr/bin/env python3
"""Export real mainnet HeaderInput records for the opt-in zkVM epoch profile."""

import argparse
import sqlite3
import struct
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
EPOCH_LENGTH = 2016


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--epochs", type=int, default=3, help="complete epochs available to the test")
    parser.add_argument("--db", type=Path, default=ROOT / "headers.db")
    parser.add_argument("--output", type=Path, default=ROOT / "fixtures/epoch_profile_headers.bin")
    args = parser.parse_args()
    if args.epochs < 1:
        parser.error("--epochs must be positive")

    count = args.epochs * EPOCH_LENGTH + 1
    with sqlite3.connect(f"file:{args.db}?mode=ro", uri=True) as db:
        rows = db.execute(
            "SELECT height, version, merkle_root, timestamp, nonce, median_time_past, ordered_timestamp_run FROM headers "
            "WHERE height BETWEEN 1 AND ? ORDER BY height", (count,)
        )
        headers = bytearray()
        hint_offsets = bytearray(struct.pack("<I", 0))
        hints = bytearray()
        previous_run = 0  # Genesis has no preceding timestamp link.
        written = 0
        for expected_height, (height, version, merkle_root, timestamp, nonce, median, run) in enumerate(rows, 1):
            if height != expected_height or len(merkle_root) != 32:
                raise ValueError(f"invalid header at height {expected_height}")
            headers.extend(struct.pack("<I32sII", version, merkle_root, timestamp, nonce))
            if height >= 11 and previous_run < 10:
                hints.extend(struct.pack("<I", median))
            hint_offsets.extend(struct.pack("<I", len(hints) // 4))
            previous_run = run
            written = expected_height
        if written != count:
            raise ValueError(f"database contains only {written} of {count} requested headers")
        with args.output.open("wb") as output:
            output.write(struct.pack("<I", count))
            output.write(headers)
            output.write(hint_offsets)
            output.write(hints)
    print(f"wrote {count} mainnet headers to {args.output}")


if __name__ == "__main__":
    main()
