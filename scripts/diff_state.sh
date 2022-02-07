#!/bin/bash
# Computes a unified diff between the provided left branch / tag state.rs and the right branch / tag state.rs.
# Falls back to the most recent version tag for left.
# Falls back to the current branch for right.
# Requires a pristine repo for safety.

set -o errexit
command -v shellcheck >/dev/null && shellcheck "$0"

function generate_state() {
  for S in */*/src/state.rs; do
    DIR=$(dirname "$S")/..
    (cd "$DIR"; mkdir -p state/; cp src/state.rs state/)
  done
}

LEFT_TAG="$1"
[ -z "$LEFT_TAG" ] && LEFT_TAG=$(git tag --sort=creatordate | grep -E '^v[0-9]+\.[0-9]+\.[0-9]+' | tail -1)

if [ "$LEFT_TAG" = "-h" ] || [ "$LEFT_TAG" = "--help" ]
then
  echo "Usage: $0 [-j] [LEFT_TAG] [RIGHT_TAG]"
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

RESULTS="./state_diffs-$LEFT_TAG-$RIGHT_TAG.txt"

# Remove state on exit
trap 'rm -rf ./*/*/"state-$LEFT_TAG" ./*/*/"state-$RIGHT_TAG"' 0

# Generate LEFT_TAG state
git checkout "$LEFT_TAG" || echo "Error: cannot checkout to $LEFT_TAG." || exit 1
generate_state

# Move it
for S in */*/state
do
  mv "$S" "$S-$LEFT_TAG"
done

# Generate RIGHT_TAG state
git checkout "$RIGHT_TAG" || echo "Error: cannot checkout to $RIGHT_TAG." || exit 1
generate_state

# Move it
for S in */*/state
do
  mv "$S" "$S-$RIGHT_TAG"
done

# Compare it
for SL in */*/"state-$LEFT_TAG"
do
  PARENT=$(dirname "$SL")
  echo "$PARENT":
  SR="$PARENT/state-$RIGHT_TAG"
  diff -u <(sed -E '{N; /^\s*#\[cfg\(test\)\]\s+mod\s+[a-z_]+\s*\{/,$d}' "$SL"/state.rs) <(sed -E '{N; /^\s*#\[cfg\(test\)\]\s+mod\s+[a-z_]+\s*\{/,$d}' "$SR"/state.rs) | { grep -v '^[+-]\s*\/[\/\*]' || true; }
done >"$RESULTS"

# Return to current branch
git checkout "$CURRENT_TAG"

echo
echo "State diffs in $RESULTS."
