"""``python -m marg``: batch, collect and report without the Rust CLI binary.

``batch`` and ``collect`` call the same core functions as ``marg batch`` and
``marg collect``; ``report`` renders the Quarto reports (``marg report``
delegates here).
"""

from __future__ import annotations

import argparse
import json
import sys

from marg import _core


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(prog="python -m marg")
    sub = ap.add_subparsers(dest="cmd", required=True)
    b = sub.add_parser("batch", help="run entries of an experiment's run list")
    b.add_argument("experiment")
    b.add_argument("--start", type=int)
    b.add_argument("--count", type=int)
    b.add_argument("--ids", type=lambda s: [int(x) for x in s.split(",") if x])
    b.add_argument("--chunk", type=int)
    b.add_argument("--chunk-size", type=int)
    b.add_argument("--threads", type=int, default=1)
    b.add_argument("--force", action="store_true")
    b.add_argument("--params")
    c = sub.add_parser("collect", help="validate run folders, write index.parquet")
    c.add_argument("experiment")
    c.add_argument("--allow-incomplete", action="store_true")
    c.add_argument("--clean-partials", action="store_true")
    r = sub.add_parser("report", help="render a report to HTML and PDF")
    r.add_argument(
        "kind", choices=["run", "experiment", "sensitivity", "calibration-stage", "params"]
    )
    r.add_argument("path")
    r.add_argument("--out", help="output folder (default: <path>/report)")
    r.add_argument("--format", action="append", choices=["html", "pdf"])
    r.add_argument("--metric", action="append", help="KPI columns to analyse")
    a = ap.parse_args(argv)

    if a.cmd == "batch":
        start, count = a.start, a.count
        if a.chunk is not None:
            if not a.chunk_size:
                ap.error("--chunk needs --chunk-size")
            start, count = a.chunk * a.chunk_size, a.chunk_size
        out = _core.run_batch(a.experiment, start, count, a.ids, a.threads, a.force, a.params)
        print(out)
        return 1 if json.loads(out)["failed"] else 0
    if a.cmd == "collect":
        rep = json.loads(_core.collect(a.experiment, a.clean_partials))
        print(json.dumps(rep, indent=2))
        complete = rep["ok"] == rep["total"] and not rep["duplicate_paths"]
        return 0 if complete or a.allow_incomplete else 1
    from marg import report

    paths = report.render(a.kind, a.path, out=a.out, formats=a.format, metrics=a.metric)
    for p in paths:
        print(p)
    return 0


if __name__ == "__main__":
    sys.exit(main())
