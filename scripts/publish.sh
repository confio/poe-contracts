#!/bin/bash
set -o errexit -o nounset -o pipefail
command -v shellcheck > /dev/null && shellcheck "$0"

# these are imported by other packages - wait 30 seconds between each as they have linear dependencies
BASE_PACKAGES="bindings bindings-test tg4 utils"
ALL_PACKAGES="voting-contract"

# these are imported by other contracts
BASE_CONTRACTS="tg4-engagement tg4-mixer tg4-stake"
# Not yet ready but could be BASE_CONTRACTS tgrade-gov-reflect tgrade-vesting-account"
ALL_CONTRACTS="tgrade-community-pool tgrade-validator-voting tgrade-valset"

SLEEP_TIME=30

for pack in $BASE_PACKAGES; do
  (
    cd "packages/$pack"
    echo "Publishing $pack"
    cargo publish
    # wait for these to be processed on crates.io
    echo "Waiting for crates.io to recognize $pack"
    sleep $SLEEP_TIME
  )
done

for pack in $ALL_PACKAGES; do
  (
    cd "packages/$pack"
    echo "Publishing $pack"
    cargo publish
  )
done

# wait for these to be processed on crates.io
echo "Waiting for publishing all packages"
sleep $SLEEP_TIME

for cont in $BASE_CONTRACTS; do
  (
    cd "contracts/$cont"
    echo "Publishing $cont"
    cargo publish
  )
done

# wait for these to be processed on crates.io
echo "Waiting for publishing base contracts"
sleep $SLEEP_TIME

for cont in $ALL_CONTRACTS; do
  (
    cd "contracts/$cont"
    echo "Publishing $cont"
    cargo publish
  )
done

echo "Everything is published!"
