#!/usr/bin/env bash
# Regenerate the JSON Schema from the Rust types, then the Python and TypeScript
# types from it. CI runs this and fails if anything changed (plan P0.4).
set -euo pipefail
cd "$(dirname "$0")/.."

cargo run -q -p marg-cli -- schema export --out-dir schema

uv run -q datamodel-codegen \
  --input schema/scenario.schema.json \
  --input-file-type jsonschema \
  --output python/marg/_schema.py \
  --output-model-type pydantic_v2.BaseModel \
  --target-python-version 3.11 \
  --use-standard-collections \
  --use-union-operator \
  --disable-timestamp \
  --formatters black isort \
  --custom-file-header '# Generated from schema/scenario.schema.json by scripts/gen-schema.sh. Do not edit.'

(cd web && npm run -s gen:schema)
echo "schema, python/marg/_schema.py and web/src/schema.ts regenerated"
