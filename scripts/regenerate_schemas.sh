#!/bin/bash
set -o errexit -o nounset -o pipefail
command -v shellcheck > /dev/null && shellcheck "$0"

for C in contracts/*/; do
  (cd "$C"; cargo schema)
done

for S in packages/*/examples/schema.rs; do
  P=$(dirname "$S")/..
  (cd "$P"; cargo schema)
done
