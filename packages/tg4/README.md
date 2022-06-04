# TG4 Spec: Group Members

Based on [cw-plus](https://github.com/CosmWasm/cw-plus)
[CW4](https://github.com/CosmWasm/cw-plus/tree/master/packages/cw4).

TG4 is a spec for storing group membership, which can be combined
with [TG3](https://github.com/confio/poe-contracts/tree/main/packages/tg3) multisigs.
The purpose is to store a set of members/voters that can be accessed
to determine permissions in another section.

Since this is often deployed as a contract pair, we expect this
contract to often be queried with `QueryRaw` and the internal
layout of some data structures becomes part of the public API.
Implementations may add more data structures, but at least
the ones laid out here should be under the specified keys and in the
same format.

In this case, a tg3 contract could *read* an external group contract with
no significant cost more than reading local storage. However, updating
that group contract (if allowed), would be an external message and
charged the instantiation overhead for each contract.

## Messages

We define an `InstantiateMsg{admin, members}` to make it easy to set up a group
as part of another flow. Implementations should work with this setup,
but may add extra `Option<T>` fields for non-essential extensions to
configure in the `instantiate` phase.

There are three messages supported by a group contract:

`UpdateAdmin{admin}` - changes (or clears) the admin for the contract

`AddHook{addr}` - adds a contract address to be called upon every
  `UpdateMembers` call. This can only be called by the admin, and care must
  be taken. A contract returning an error or running out of gas will
  revert the membership change (see more in Hooks section below).

`RemoveHook{addr}` - unregister a contract address that was previously set
  by `AddHook`.

Only the `admin` may execute any of these function. Thus, by omitting an
`admin`, we end up with a similar functionality than `cw3-fixed-multisig`.
If we include one, it may often be desired to be a `tg3` contract that
uses this group contract as a group. This leads to a bit of chicken-and-egg
problem, but we cover how to instantiate that in
[`cw3-flexible-multisig`](https://github.com/CosmWasm/cosmwasm-plus/tree/master/contracts/cw3-flexible-multisig/README.md#instantiation).

## Queries

### Smart

`TotalPoints{}` - Returns the total points of all current members,
  this is very useful if some conditions are defined on a "percentage of members".

`Member{addr, height}` - Returns the points of this voter if they are a member of the
  group (may be 0), or `None` if they are not a member of the group.
  If height is set, and the tg4 implementation supports snapshots,
  this will return the points of that member at
  the beginning of the block with the given height.

`ListMembers{start_after, limit}` - Allows us to paginate over the list
   of all members. 0-points members will be included. Removed members will not.

`ListMembersByPoints{start_after, limit}` - Allows us to paginate over the list
   of members, sorted by descending points.

`Admin{}` - Returns the `admin` address, or `None` if unset.

### Raw

In addition to the above "SmartQueries", which make up the public API,
we define two raw queries that are designed for more efficiency
in contract-contract calls. These use keys exported by `tg4`

`TOTAL_KEY` - making a raw query with this key (`b"total"`) will return a
  JSON-encoded `u64`

`member_key()` - takes an `Addr` and returns a key that can be
   used for raw query (`"\x00\x07members" || addr`). This will return
   empty bytes if the member is not inside the group, otherwise a
   JSON-encoded `MemberInfo` struct, that contains the member points
   and optionally their membership start height. Which can be used
   for tie breaking between members with the same number of points.
   See [query.rs](./src/query.rs) for details.

## Hooks

One special feature of `tg4` contracts is that they allow the admin to
register multiple hooks. These are special contracts that need to react
to changes in the group membership, and this allows them stay in sync.
Again, note this is a powerful ability, and you should only set hooks
to contracts you fully trust; generally some contracts you deployed
alongside the group.

If a contract is registered as a hook on a tg4 contract, then anytime
`UpdateMembers` is successfully executed, the hook will receive a `handle`
call with the following format:

```json
{
  "member_changed_hook": {
    "diffs": [
      {
        "addr": "cosmos1y3x7q772u8s25c5zve949fhanrhvmtnu484l8z",
        "old_points": 20,
        "new_points": 24
      }
    ]
  }
}
```

See [hook.rs](./src/hook.rs) for full details. Note that this example
shows an update or an existing member. `old_points` will
be missing if the address was added for the first time. And
`new_points` will be missing if the address was removed.

The receiving contract must be able to handle the `MemberChangedHookMsg`
and should only return an error if it wants to change the functionality
of the group contract (e.g. a multisig that wants to prevent membership
changes while there is an open proposal). However, such cases are quite
rare and often point to fragile code.

Note that the message sender will be the group contract that was updated.
Make sure you check this when handling, so external actors cannot
call this hook, only the trusted group.