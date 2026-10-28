"""Export one player's path-conditioned range in Desktop Postflop syntax."""

import argparse
from decimal import Decimal, ROUND_HALF_UP, localcontext
from fractions import Fraction
import json
from pathlib import Path
import sqlite3

DATABASE = Path(__file__).resolve().parents[1] / "data/preflop/bansolver-ranges.sqlite3"
POSITIONS = {"EP": 3, "MP": 2, "CO": 1, "BTN": 0, "SB": 9, "BB": 8}


def export(db, dataset, path, position):
    if not db.execute("SELECT 1 FROM datasets WHERE id=?", (dataset,)).fetchone():
        raise ValueError(f"Unknown dataset: {dataset}")
    if not db.execute("SELECT 1 FROM nodes WHERE dataset_id=? AND path=?", (dataset, path)).fetchone():
        raise ValueError("The requested path does not exist in this dataset")
    hands = [r[0] for r in db.execute("SELECT hand FROM hands WHERE dataset_id=? ORDER BY hand_index", (dataset,))]
    weights = [Fraction(1) for _ in hands]
    node_path = ""
    for action in path.split("/") if path else []:
        row = db.execute("SELECT actor,frequency_35ths_json FROM actions WHERE dataset_id=? AND path=? AND name=?", (dataset, node_path, action)).fetchone()
        if row is None:
            raise ValueError(f"No strategy for action {action!r} at {node_path!r}")
        actor, encoded = row
        if actor == POSITIONS[position]:
            if action == "Fold":
                raise ValueError(f"{position} folded on this path")
            weights = [weight * Fraction(frequency, 35) for weight, frequency in zip(weights, json.loads(encoded), strict=True)]
        node_path = f"{node_path}/{action}" if node_path else action
    parts = []
    with localcontext() as context:
        context.prec = 40
        for hand, weight in zip(hands, weights, strict=True):
            rounded = (Decimal(weight.numerator) / Decimal(weight.denominator)).quantize(Decimal("0.000001"), rounding=ROUND_HALF_UP)
            if rounded == 1:
                parts.append(hand)
            elif rounded > 0:
                parts.append(f"{hand}:{rounded.normalize():f}")
    return ",".join(parts)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("dataset", choices=("gg-zoom-2bb", "gg-zoom-2p5bb"))
    parser.add_argument("position", choices=POSITIONS)
    parser.add_argument("path", help="Exact action path, for example 'Fold/Fold/Raise 2 bb/Fold/Fold/Call'")
    args = parser.parse_args()
    try:
        with sqlite3.connect(f"file:{DATABASE}?mode=ro", uri=True) as db:
            result = export(db, args.dataset, args.path, args.position)
        if not result:
            parser.error("The resulting range is empty after rounding")
        print(result)
    except (ValueError, sqlite3.Error) as error:
        parser.error(str(error))


if __name__ == "__main__":
    main()
