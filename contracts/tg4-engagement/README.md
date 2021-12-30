# TG4 Group

This is a basic implementation of the [tg4 spec](../../packages/tg4/README.md).
It fulfills all elements of the spec, including the raw query lookups,
and it designed to be used as a backing storage for
[cw3 compliant contracts](https://github.com/CosmWasm/cosmwasm-plus/blob/master/packages/cw3/README.mdl).

It stores a set of members along with an admin, and allows the admin to
update the state. Raw queries (intended for cross-contract queries)
can check a given member address and the total weight. Smart queries (designed
for client API) can do the same, and also query the admin address as well as
paginate over all members.

Also this contract provides api for behavior similar to EIP2222 standard, which
allows for automatic split tokens sent to this contract proportionally to
members weights.

## Init

To create it, you must pass in a list of members, as well as an optional
`admin`, if you wish it to be mutable.

```rust
pub struct InstantiateMsg {
    pub admin: Option<HumanAddr>,
    pub members: Vec<Member>,
    pub preauths: Option<u64>,
    pub halflife: Option<Duration>,
    pub token: Option<String>,
}

pub struct Member {
    pub addr: HumanAddr,
    pub weight: u64,
}
```

Members are defined by an address and a weight. This is transformed
and stored under their `CanonicalAddr`, in a format defined in
[tg4 raw queries](../../packages/tg4/README.md#raw).

Note that 0 *is an allowed weight*. This doesn't give any voting rights, but
it does define this address is part of the group. This could be used in
e.g. a KYC whitelist to say they are allowed, but cannot participate in
decision-making.

`token` is a native token name which may be distributed with EIP2222-like
interface. If it is `None`, no tokens may be distributed by this contract.

## Messages

Basic update messages, queries, and hooks are defined by the
[tg4 spec](../../packages/tg4/README.md). Please refer to it for more info.

`tg4-engagement` adds one message to control the group membership:

`UpdateMembers {add, remove}` - takes a membership diff and adds/updates the
members, as well as removing any provided addresses. If an address is on both
lists, it will be removed. If it appears multiple times in `add`, only the
last occurrence will be used.

`AddHook {addr}` - adds a new hook to be informed of all membership changes.
Must be called by an Admin.

`RemoveHook {addr}` - removes a hook. Must be called by an Admin.

`DistributeFunds {sender}` - distributes funds sent with this message, and sent with
regular bank message since last `DistributeFunds`. `sender` is optional info
overwriting `sender` field on generated event. Funds are distributed to members,
proportionally to their weights. Funds are not sent to members directly, instead
they are assigned for future withdrawal.

`WithdrawFunds {receiver}` - withdraws funds previously assigned to sender of the
message while funds distribution. Optional `receiver` field is an address where
funds should be send, message sender by default.

`DelegateWithdrawal{delegated}` - set `delegated` address to be allowed to
withdraw funds assigned to `sender`. Only one address can be delegated for any
address, so delegating new address overwrites previous one. To disallow any
address to withdraw funds, send `DelegateWithdrawal` with `delegated` send
to `sender`.

## Queries

`Hooks {}` - returns all registered hooks.

`Preauths {}` - returns the current number of preauths.

`WithdrawableFunds {owner}` - returns how much funds is assigned for withdrawal by
owner.

`DistributedFunds {}` - returns how much funds were distributed by this contract in
its lifetime.

`UndistributedFunds {}` - returns how much funds is waiting for distribution on this
contract.

`Delegated {owner}` - returns address allowed to withdraw funds assigned to given
`owner`. If none is set, `owner` would be returned.
