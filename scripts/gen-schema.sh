#!/usr/bin/env bash
# Regenerate the JSON Schema from the Rust types, then the Python and TypeScript
# types from it. CI runs this and fails if anything changed (plan P0.4).
set -euo pipefail
cd "$(dirname "$0")/.."

cargo run -q -p marg-cli -- schema export --out-dir schema

gen_py() { # schema output
uv run -q datamodel-codegen \
  --input "$1" \
  --input-file-type jsonschema \
  --output "$2" \
  --output-model-type pydantic_v2.BaseModel \
  --target-python-version 3.11 \
  --use-standard-collections \
  --use-union-operator \
  --disable-timestamp \
  --formatters black isort \
  --custom-file-header "# Generated from $1 by scripts/gen-schema.sh. Do not edit."
}
gen_py schema/scenario.schema.json python/marg/_schema.py
gen_py schema/register_file.schema.json python/marg/_register_schema.py

(cd web && npm run -s gen:schema)
echo "schema/, python/marg/_*schema.py and web/src/{schema,register}.ts regenerated"
