# TG4 Group

This is a basic implementation of the [tg4 spec](../../packages/tg4/README.md).
It fulfills all elements of the spec, including the raw query lookups,
and it designed to be used as a backing storage for
[cw3 compliant contracts](../../packages/cw3/README.md).

It stores a set of members along with an admin, and allows the admin to
update the state. Raw queries (intended for cross-contract queries)
can check a given member address and the total points. Smart queries (designed
for client API) can do the same, and also query the admin address as well as
paginate over all members.

## Init

To create it, you must pass in a list of members, as well as an optional
`admin`, if you wish it to be mutable.

```rust
pub struct InitMsg {
    pub admin: Option<HumanAddr>,
    pub members: Vec<Member>,
}

pub struct Member {
    pub addr: HumanAddr,
    pub points: u64,
}
```

Members are defined by an address and a number of points. This is transformed
and stored under their `CanonicalAddr`, in a format defined in
[tg4 raw queries](../../packages/tg4/README.md#raw).

Note that 0 *is an allowed number of points*. This doesn't give any voting rights, but
it does define this address is part of the group. This could be used in
e.g. a KYC whitelist to say they are allowed, but cannot participate in
decision-making.

## Messages

Basic update messages, queries, and hooks are defined by the
[tg4 spec](../../packages/tg4/README.md). Please refer to it for more info.

`tg4-group` adds one message to control the group membership:

`UpdateMembers{add, remove}` - takes a membership diff and adds/updates the
members, as well as removing any provided addresses. If an address is on both
lists, it will be removed. If it appears multiple times in `add`, only the
last occurrence will be used.

