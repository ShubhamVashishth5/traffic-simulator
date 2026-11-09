#!/usr/bin/env bash
# Run the core criterion benches on two git revisions on the same machine and
# fail if any benchmark got more than THRESHOLD percent slower (plan P0.7).
# Usage: scripts/bench-compare.sh <base-rev> <head-rev>
set -euo pipefail
BASE=${1:?base revision}
HEAD_REV=${2:-HEAD}
THRESHOLD=${THRESHOLD:-5}
cd "$(dirname "$0")/.."
ROOT=$(pwd)
WORK=$(mktemp -d)
trap 'git worktree remove --force "$WORK/base" >/dev/null 2>&1 || true; rm -rf "$WORK"' EXIT

run_bench() { # dir name
  (cd "$1" && CARGO_TARGET_DIR="$ROOT/target" cargo bench -q -p marg-core --bench core -- \
     --save-baseline "$2" --noplot >/dev/null)
}

git worktree add -q "$WORK/base" "$BASE"
if [ -d "$WORK/base/crates/marg-core/benches" ]; then
  run_bench "$WORK/base" base
else
  echo "base has no benches; skipping comparison"; exit 0
fi
git checkout -q "$HEAD_REV"
run_bench "$ROOT" head

python3 - "$THRESHOLD" <<'PY'
import json, pathlib, sys
threshold = float(sys.argv[1])
root = pathlib.Path("target/criterion")
worst, failed = 0.0, []
for est in sorted(root.rglob("head/estimates.json")):  # grouped benches nest deeper
    name = str(est.parent.parent.relative_to(root))
    base = est.parent.parent / "base" / "estimates.json"
    if not base.exists():
        print(f"{name}: new benchmark"); continue
    h = json.loads(est.read_text())["median"]["point_estimate"]
    b = json.loads(base.read_text())["median"]["point_estimate"]
    change = 100 * (h - b) / b
    print(f"{name}: base {b/1e6:.3f} ms, head {h/1e6:.3f} ms, {change:+.1f}%")
    worst = max(worst, change)
    if change > threshold:
        failed.append(name)
if failed:
    sys.exit(f"slower than {threshold}%: {', '.join(failed)}")
print(f"ok (worst change {worst:+.1f}%)")
PY
