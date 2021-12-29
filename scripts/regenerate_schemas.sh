#!/bin/bash
set -o errexit -o nounset -o pipefail
command -v shellcheck > /dev/null && shellcheck "$0"

for contract in contracts/*; do
  (cd "$contract"; cargo schema; cd -)
done

for package in packages/*; do
  if ! (echo "$package" | grep -q controllers); then
    (cd "$package"; cargo schema; cd -)
  fi
done
