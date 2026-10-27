"""Audit and compact pkcore's exact HU table, preserving relative suits.

Usage: python3 scripts/convert_preflop_table.py /tmp/pain2note-hups.db
Source revision and SHA-256 are recorded in docs/equity-sources.md.
Input is read-only. Every physical matchup and all suit isomorphisms are checked.
"""
import hashlib
import itertools
from pathlib import Path
import sqlite3
import struct
import sys

source = Path(sys.argv[1]).resolve()
conn = sqlite3.connect(f"file:{source}?mode=ro", uri=True)
perms = list(itertools.permutations(range(4)))
transforms = {}
for a, b in itertools.combinations(range(52), 2):
    variants = []
    for perm in perms:
        x, y = sorted((perm[a // 13] * 13 + a % 13, perm[b // 13] * 13 + b % 13))
        variants.append(x * 52 + y)
    transforms[(1 << a) | (1 << b)] = variants

canonical = {}
origins = {}
physical = set()
for higher, lower, wins, losses, ties in conn.execute(
    "SELECT higher, lower, higher_wins, lower_wins, ties FROM nlh_headsup_result"
):
    assert higher in transforms and lower in transforms and not higher & lower
    assert min(wins, losses, ties) >= 0 and wins + losses + ties == 1_712_304
    matchup = tuple(sorted((higher, lower)))
    assert matchup not in physical, "Duplicate physical matchup"
    physical.add(matchup)
    key, flipped = min(
        ((min(a, b) << 12) | max(a, b), a > b)
        for a, b in zip(transforms[higher], transforms[lower])
    )
    counts = (losses, wins, ties) if flipped else (wins, losses, ties)
    if key in canonical:
        assert canonical[key] == counts, f"Suit symmetry mismatch at key {key}: {canonical[key]} != {counts}; source rows {origins[key]} vs {(higher, lower, wins, losses, ties)}"
    canonical[key] = counts
    origins[key] = (higher, lower, wins, losses, ties)

assert len(physical) == 812_175, "Incomplete physical matchup coverage"
assert len(canonical) == 47_086, f"Unexpected canonical coverage: {len(canonical)}"
destination = Path(__file__).resolve().parents[1] / "src-tauri/data/preflop-hu.bin"
destination.parent.mkdir(parents=True, exist_ok=True)
data = b"".join(struct.pack("<IIII", key, *counts) for key, counts in sorted(canonical.items()))
destination.write_bytes(data)
print(f"Verified {len(physical):,} physical matchups -> {len(canonical):,} canonical matchups")
print(f"Source SHA-256: {hashlib.sha256(source.read_bytes()).hexdigest()}")
print(f"Output SHA-256: {hashlib.sha256(data).hexdigest()}; {len(data):,} bytes")
