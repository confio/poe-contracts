#!/bin/bash
# Computes a unified diff between the provided left branch / tag schemas and the right branch / tag schemas.
# Falls back to the most recent version tag for left.
# Falls back to the current branch for right.
# Requires a pristine repo for safety.

set -o errexit -o pipefail
command -v shellcheck >/dev/null && shellcheck "$0"

function generate_schemas() {
  for C in contracts/*/; do
    (cd "$C"; cargo schema)
  done

  for S in packages/*/examples/schema.rs; do
    P=$(dirname "$S")/..
    (cd "$P"; cargo schema)
  done
}

LEFT_TAG="$1"
[ -z "$LEFT_TAG" ] && LEFT_TAG=$(git tag --sort=creatordate | grep -E '^v[0-9]+\.[0-9]+\.[0-9]+' | tail -1)

if [ "$LEFT_TAG" = "-h" ] || [ "$LEFT_TAG" = "--help" ]
then
  echo "Usage: $0 [LEFT_TAG] [RIGHT_TAG]"
  echo "Left tag default: Most recent version tag."
  echo "Right tag default: Current branch."
  exit 1
fi

CURRENT_TAG=$(git rev-parse --abbrev-ref HEAD)

RIGHT_TAG="$2"
[ -z "$RIGHT_TAG" ] && RIGHT_TAG=$CURRENT_TAG

echo "Git left version tag: $LEFT_TAG"
echo "Git right version tag: $RIGHT_TAG"
echo


# Check for pristine repo (ignoring untracked files)
[ -n "$(git status --porcelain --untracked-files=no)" ] && echo "Uncommitted changes in working copy. Aborting." && exit 1

RESULTS="./schema_diffs-$LEFT_TAG-$RIGHT_TAG.txt"

# Remove schemas on exit
trap 'rm -rf ./*/*/"schema-$LEFT_TAG" ./*/*/"schema-$RIGHT_TAG"' 0

# Generate LEFT_TAG schemas
git checkout "$LEFT_TAG" || echo "Error: cannot checkout to $LEFT_TAG." || exit 1
generate_schemas

# Move them
for S in */*/schema
do
  mv "$S" "$S-$LEFT_TAG"
done

# Generate RIGHT_TAG schemas
git checkout "$RIGHT_TAG" || echo "Error: cannot checkout to $RIGHT_TAG." || exit 1
generate_schemas

# Move them
for S in */*/schema
do
  mv "$S" "$S-$RIGHT_TAG"
done

# Compare them
for SL in */*/"schema-$LEFT_TAG"
do
  echo "$SL":
  SR=$(dirname "$SL")"/schema-$RIGHT_TAG"
  diff -u "$SL" "$SR"
done >"$RESULTS"

# Return to current branch
git checkout "$CURRENT_TAG"

echo
echo "Schema diffs in $RESULTS."
