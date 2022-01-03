# Tgrade Community Pool contract

This defines and implements Community Pool functionality for managing
engagement points and validator slashing, according to proposals voting,
based on [CW3](https://github.com/CosmWasm/cosmwasm-plus/tree/master/packages/cw3).

## Instantiation

The first step to create an tgrade-community-pool contract is to instantiate
a tg4 contract with the desired member set.

We want to put some block rewards into the hands of the community via a
community pool, in order to provide more engagement and to decentralize the
chain more.

The engagement point holders can make spending proposals, which can be voted on
by engagement holders.

## Rewards split

5% of all rewards go to this new community pool contract (47.5% to validators,
47.5% to engagement holders).

The community pool contract is another voting contract like "validator voting",
but the voting set is based on engagement point holders.

This contract should accept distribution messages, the same interface as others.

Votes set to 55% threshold, 40% quorum, and 14 day voting period.

None of this is hardcoded - those are values intended to be used in final tgrade
chain.

## Execution Process

First, a registered voter must submit a proposal. This also includes the
first "Yes" vote on the proposal by the proposer. The proposer can set
an expiration time for the voting process, or it defaults to the limit
provided when creating the contract (so proposals can be closed after several
days).

Before the proposal has expired, any voter with non-zero weight can add their
vote. Only "Yes" votes are tallied. If enough "Yes" votes were submitted before
the proposal expiration date, the status is set to "Passed".

Once a proposal is "Passed", anyone may submit an "Execute" message. This will
trigger the proposal to send all stored messages from the proposal and update
it's state to "Executed", so it cannot run again. (Note if the execution fails
for any reason - out of gas, insufficient funds, etc - the state update will
be reverted, and it will remain "Passed", so you can try again).

Once a proposal has expired without passing, anyone can submit a "Close"
message to mark it closed. This has no effect beyond cleaning up the UI/database.

TODO: this contract currently assumes the group membership is static during
the lifetime of one proposal. If the membership changes when a proposal is
open, this will calculate incorrect values (future PR).

## Running this contract

You will need Rust 1.53.0+ with `wasm32-unknown-unknown` target installed.

You can run unit tests on this via:

`cargo test`

Once you are happy with the content, you can compile it to wasm via:

```
RUSTFLAGS='-C link-arg=-s' cargo wasm
cp ../../target/wasm32-unknown-unknown/release/tgrade_oc_proposals.wasm .
ls -l tgrade_oc_proposals.wasm
sha256sum tgrade_oc_proposals.wasm
```

Or for a production-ready (optimized) build, run a build command in
the repository root: https://github.com/CosmWasm/cw-plus#compiling.
