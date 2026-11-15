#!/usr/bin/env bash
# Print the CHANGELOG.md section of a version (the body of its GitHub release), or fail if
# there is none. Usage: scripts/release-notes.sh [version]  (default: the workspace version in
# Cargo.toml; a leading "v" is accepted). With LINK_BASE set (e.g.
# https://github.com/<owner>/<repo>/blob/v0.1.0), relative links become absolute ones under it,
# since a release body has no folder to resolve them against.
set -euo pipefail
cd "$(dirname "$0")/.."
v=${1:-$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)}
v=${v#v}
notes=$(awk -v head="## [$v]" '
  index($0, head) == 1 { on = 1; next }
  on && /^## \[/ { exit }
  on && /^\[[^]]+\]: / { next }
  on { print }
' CHANGELOG.md)
if ! grep -q '[^[:space:]]' <<<"$notes"; then
  echo "CHANGELOG.md has no section \"## [$v]\": write the release notes first" >&2
  exit 1
fi
if [ -n "${LINK_BASE:-}" ]; then
  notes=$(sed -E "s#\]\(([^)#:]+)\)#](${LINK_BASE%/}/\1)#g" <<<"$notes")
fi
printf '%s\n' "$notes"
