# TG4 Mixer

This mixes two tg4 contracts.
On init, you pass addresses to two tg4 contracts, and this one will
register a listening hook on both. Following that, it will query both
for their current state and use a mixing function to calculate the combined value.
(We currently implement/optimized it with the assumption that `None` points in
either upstream group means a `None` points in this group)

Every time one of the upstream contracts changes, it will use the mixing
function again to recalculate the combined points of the affected addresses.

Apart from tg4, both upstream contracts need to implement the slashing API.

## Init

To create it, you must pass in the two groups you want to listen to.
We must be pre-authorized to self-register as a hook listener on both of them.

```rust
pub struct InitMsg {
    pub left_group: String,
    pub right_group: String,
}
```

## Mixing Function

As mentioned above, we optimize for the case where `None` on either
contract leads to `None` in the combined group. This is especially used
for the initialization.

A number of mixing functions are implemented:
 - `GeometricMean`. A simple geometric mean of `left` and `right`.
 - `Sigmoid`. A sigmoid-like function like the one discussed in the PoE whitepaper.
 - `SigmoidSqrt`. A variant of the above, with a `p = 0.5`, and implemented using `GeometricSigmoid`.
 - `AlgebraicSigmoid`. An algebraic sigmoid modelled after `Sigmoid`.

## Updates

Basic messages, queries, and hooks are defined by the
[tg4 spec](../../packages/tg4/README.md). Please refer to it for more info.

We just add `ExecuteMsg::MemberChangedHook` to listen for changes on the
upstream contracts.

## Benchmarking

```
cd contracts/tg4-mixer
cargo bench --features benches
```
