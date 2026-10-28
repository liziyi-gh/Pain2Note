"""Convert the saved BanSolver trees into a queryable, lossless SQLite database."""

import hashlib
import json
from pathlib import Path
import sqlite3
import tempfile
import os

ROOT = Path(__file__).resolve().parents[1]
DATA = ROOT / "data" / "preflop"
SOURCES = {
    "gg-zoom-2bb": ("gg-zoom-2bb.json", "3271cb6746f789481024f2875f5faaa641d9c617ee06e0c9626c7019951304e1"),
    "gg-zoom-2p5bb": ("gg-zoom-2p5bb.json", "9542f042696fd9f53df4ebe26b76764a42a6525482e8a82db40b07a20fce6737"),
}
OUTPUT = DATA / "bansolver-ranges.sqlite3"


def integer36(value):
    if not value or any(c not in "0123456789abcdefghijklmnopqrstuvwxyz" for c in value):
        raise ValueError(f"Invalid base36 encoding: {value!r}")
    return int(value, 36)


def compact(value):
    return json.dumps(value, ensure_ascii=False, separators=(",", ":"))


def main():
    handle, temporary = tempfile.mkstemp(prefix=".ranges-", suffix=".sqlite3", dir=DATA)
    os.close(handle)
    try:
        with sqlite3.connect(temporary) as db:
            db.executescript("""
                PRAGMA foreign_keys = ON;
                PRAGMA user_version = 1;
                CREATE TABLE datasets (
                    id TEXT PRIMARY KEY, source_file TEXT NOT NULL,
                    source_sha256 TEXT NOT NULL, metadata_json TEXT NOT NULL
                ) WITHOUT ROWID;
                CREATE TABLE hands (
                    dataset_id TEXT NOT NULL REFERENCES datasets(id),
                    hand_index INTEGER NOT NULL, hand TEXT NOT NULL,
                    PRIMARY KEY (dataset_id, hand_index), UNIQUE(dataset_id, hand)
                ) WITHOUT ROWID;
                CREATE TABLE nodes (
                    dataset_id TEXT NOT NULL REFERENCES datasets(id),
                    path TEXT NOT NULL, previous_actor INTEGER NOT NULL,
                    terminal_x INTEGER, fold_ev_twentieths INTEGER, ev_twentieths_json TEXT,
                    PRIMARY KEY(dataset_id, path)
                ) WITHOUT ROWID;
                CREATE TABLE actions (
                    dataset_id TEXT NOT NULL, path TEXT NOT NULL, action_index INTEGER NOT NULL,
                    name TEXT NOT NULL, actor INTEGER NOT NULL, frequency_35ths_json TEXT NOT NULL,
                    PRIMARY KEY(dataset_id, path, action_index),
                    UNIQUE(dataset_id, path, name),
                    FOREIGN KEY(dataset_id, path) REFERENCES nodes(dataset_id, path)
                ) WITHOUT ROWID;
                CREATE VIEW hand_actions AS
                SELECT a.dataset_id, a.path, a.action_index, a.name AS action,
                       a.actor, h.hand, CAST(j.value AS INTEGER) AS frequency_35ths,
                       CAST(j.value AS REAL) / 35 AS frequency,
                       CAST(json_extract(n.ev_twentieths_json, '$[' || h.hand_index || ']') AS INTEGER) AS ev_twentieths,
                       n.fold_ev_twentieths
                FROM actions a
                JOIN nodes n ON n.dataset_id = a.dataset_id AND n.path = a.path
                JOIN json_each(a.frequency_35ths_json) j
                JOIN hands h ON h.dataset_id = a.dataset_id AND h.hand_index = CAST(j.key AS INTEGER);
            """)
            for dataset_id, (filename, expected_hash) in SOURCES.items():
                raw = (DATA / filename).read_bytes()
                digest = hashlib.sha256(raw).hexdigest()
                if digest != expected_hash:
                    raise ValueError(f"Unexpected source hash for {filename}: {digest}")
                source = json.loads(raw)
                if set(source) != {"meta", "nodes"}:
                    raise ValueError(f"Unexpected source fields in {filename}")
                meta, nodes = source["meta"], source["nodes"]
                hands = meta["order"]
                if len(hands) != 169 or len(set(hands)) != 169 or len(nodes) != meta["nodes"]:
                    raise ValueError(f"Invalid hand or node count in {filename}")
                db.execute("INSERT INTO datasets VALUES (?,?,?,?)", (dataset_id, filename, digest, compact(meta)))
                db.executemany("INSERT INTO hands VALUES (?,?,?)", ((dataset_id, i, hand) for i, hand in enumerate(hands)))
                strategy_count = 0
                for path, node in nodes.items():
                    if set(node) not in ({"p"}, {"p", "x"}, {"p", "k", "a", "f", "e"}):
                        raise ValueError(f"Unexpected node fields: {dataset_id} {path}")
                    fold_ev = None
                    ev = None
                    if "a" in node:
                        strategy_count += 1
                        if len(node["e"]) != 507 or len(node["f"]) != 3:
                            raise ValueError(f"Invalid EV encoding at {path}")
                        fold_ev = integer36(node["f"]) - 17575
                        ev = [integer36(node["e"][i:i+3]) - 17575 for i in range(0, 507, 3)]
                        if [item[0] for item in node["k"]] != [item[0] for item in node["a"]]:
                            raise ValueError(f"Action mismatch at {path}")
                    db.execute("INSERT INTO nodes VALUES (?,?,?,?,?,?)", (dataset_id, path, node["p"], node.get("x"), fold_ev, compact(ev) if ev is not None else None))
                    for index, ((name, actor), (_, encoded)) in enumerate(zip(node.get("k", []), node.get("a", []))):
                        if len(encoded) != 169:
                            raise ValueError(f"Invalid frequency encoding at {path}/{name}")
                        freq = [integer36(character) for character in encoded]
                        db.execute("INSERT INTO actions VALUES (?,?,?,?,?,?)", (dataset_id, path, index, name, actor, compact(freq)))
                if strategy_count != meta["strategyNodes"]:
                    raise ValueError(f"Invalid strategy count in {filename}")
                print(f"{dataset_id}: {len(nodes)} nodes, {strategy_count} strategy nodes")
            if db.execute("PRAGMA integrity_check").fetchone()[0] != "ok" or db.execute("PRAGMA foreign_key_check").fetchone():
                raise ValueError("SQLite integrity check failed")
        os.replace(temporary, OUTPUT)
        print(f"Wrote {OUTPUT}")
    finally:
        if os.path.exists(temporary):
            os.unlink(temporary)


if __name__ == "__main__":
    main()
