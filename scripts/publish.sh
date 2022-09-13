#!/bin/bash
set -o errexit -o nounset -o pipefail
command -v shellcheck >/dev/null && shellcheck "$0"

function print_usage() {
  echo "Usage: $0 [-h|--help]"
  echo "Publishes crates to crates.io."
}

if [ $# = 1 ] && { [ "$1" = "-h" ] || [ "$1" = "--help" ] ; }
then
    print_usage
    exit 1
fi

# These are imported by other packages - wait 30 seconds between each as they have linear dependencies
BASE_CRATES="packages/bindings packages/bindings-test packages/tg4 packages/utils contracts/tg4-engagement contracts/tg4-stake contracts/tg4-mixer packages/tg3 packages/voting-contract"

ALL_CRATES="packages/test-utils contracts/tgrade-community-pool contracts/tgrade-validator-voting contracts/tgrade-valset contracts/tg4-group"

SLEEP_TIME=30

for CRATE in $BASE_CRATES; do
  (
    cd "$CRATE"
    echo "Publishing $CRATE"
    cargo publish
    # wait for these to be processed on crates.io
    echo "Waiting for crates.io to recognize $CRATE"
    sleep $SLEEP_TIME
  )
done

for CRATE in $ALL_CRATES; do
  (
    cd "$CRATE"
    echo "Publishing $CRATE"
    cargo publish
  )
done

echo "Everything is published!"
