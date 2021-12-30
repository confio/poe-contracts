# Tgrade Validator Set

This uses the [Tgrade-specific bindings](../../packages/bindings) to
allow a privileged contract to map a trusted cw4 contract to the Tendermint validator
set running the chain. Pointing to a `cw4-group` contract would implement PoA,
pointing to `cw4-stake` contract would make a pure (undelegated) PoS chain.

(Slashing and reward distributions are future work for other contracts)

## Rewards calculation

On the `Tgrade::EndBlock` sudo message this contract performs rewards calculation
and distribution for active validators for the passed epoch.

The cumulative reward value contains:
* Per epoch reward - newly minted tokens each epoch
* Fees for transactions in validated blocks

Per epoch reward is configurable in instantiation message, the `epoch_reward`
field. Fees are accumulated on the contract itself.

The epoch reward is not constant - `epoch_reward` is its base value, but it is
modified based on how many fees are accumulated. The final reward formula is:
```
cumulative_reward = max(0, epoch_rewards - fee_percentage * fees) + fees
```

The idea is, that on early epochs not so many transactions are expected, so
reward is minted to make validation profitable. However, later on when there are more
transactions, fees are enough reward for validations, so new tokens doesn't need
to be minted, so there is no actual need to introduce tokens inflation.

The reward reduction functionality can be easily disabled by setting `fee_percentage`
to `0` (which effectively makes `fee_percentage * fees` always `0`). Setting
it over `1` (or `100%`) would cause that `cumulative_reward` would diminish as fees
are growing up to the point, when `fees` would reach `epoch_reward / fee_percentage`
threshold (as from this point, no new tokens are minted, only fees are split between
validators). Setting `fee_percentage` anywhere in the range `(0; 1]` causes that
cumulative reward grow is reduced - basically up to the time when `fees` reaches
`epoch_reward / fee_percentage`, all fees are worth `(1 - fee_percentage) * fees`
(they are scalded).

The next step is splitting `cumulative_reward` into parts.
For each *distribution contract*, an address and a ratio is accepted.
`distribution_contract_ratio * cumulative_reward` is sent to each such contract using
a `distribute_funds` message, whose intention is to split this part of the rewards
between non-validators, based on their engagement.
The remaining reward tokens are sent as `validators_reward` to validators of the last epoch.
The distribution ratios need to be included in the `distribution_contracts` vector configured
in `InstantiateMsg`. The sum of these ratios needs to fit in the [0, 1] range. The vector may
be empty, in which case the whole reward ends up with the validators.

When `validators_reward` is calculated, it is split between active validators.
Active validators are up to `max_validators` validators with the highest weight,
but with at least `min_weight`. `scaling` is an optional field which allows scaling
the weight for Tendermint purposes (it should not affect reward splitting). When validators
are selected, then `cumulative_reward` is split between them, proportionally to
the validators `weight`. All of `max_validators`, `min_weight`, and `scaling` are
configurable during instantiation. Splitting of `validators_reward` is realized by
an external contract.

The default value of `fee_percentage` is `0` (so when it is not specified in the message,
the reward reduction is disabled). At Tgrade genesis, `fee_percentage` is meant
to be set to `0.5`.

## Rewards distribution contract

As stated in previous section, rewards distribution is realized by an external contract
managed by `tgrade-valset`. It is assumed to be the `tg4-engagement` contract, but
in reality it should just support the proper API, which is then used by `tgrade-valset`
(which is just a subset of `tg4-engagement`).

During valset instantiation, the rewards distribution contract is instantiated using
the message:

```json
{
  "admin": "tgrade_valset_addr",
  "denom": "epoch_reward_denom",
  "members": []
}
```

The code id of the stored rewards distribution contract is sent to valset in its instantiation message
(`rewards_code_id` field). The assigned address of the rewards distribution contract would be
emitted with a `wasm` event:

```json
{
  "_contract_addr": "valset_addr",
  "action": "tgrade-valset_instantiation",
  "rewards_contract": "rewards_contract_addr"
}
```

Additionally, the rewards contract address can be queried at any time using the
`rewards_distribution_contract {}` query.

At every epoch end, rewards would be sent to the rewards distribution contract
with the execution message:

```json
{
  "distribute_funds": {}
}
```

After this, another message would be sent to update validators and their
weights:

```json
{
  "update_members": {
    "remove": ["validator_to_be_removed"],
    "add": [{
      "addr": "validator_with_weight_updated",
      "weight": 10
    }]
  }
}
```

## Jailing

Jailing is a mechanism for temporarily disallowing operators to validate blocks.

Only one address is allowed to jail members, and it is configured in
`InstantiateMsg` as an `admin`. The idea is, that an admin is some voting contract,
which would decide about banning by some voting consensus.

Jailing a member disallows him to be a validator for incoming epochs unless he is
unjailed. There are three ways to unjail a member:

* Admin can always unjail a jailed member (unjailing via voting).
* Any member can unjail himself if the jailing period expired.
* Members can be unjailed automatically after the jailing period expired (this may be
  enabled by `InstantiateMsg::auto_unjail` flag).

The status of jailing can be queried by normal validators queries - if a validator
is jailed, the response will contain a `jailed_until` field with either a single
`forever` field (if this member will never be allowed to unjail himself),
or an `until` field containing a timestamp, indicating since when the member can be unjailed.

## Slashing

The contract implements slashing semantics, but doesn't actually implement the
full slashing interface. It reacts properly to the slash message:
```json
{
  "slash": {
    "addr": "contract_to_slash",
    "portion": portion_to_slash
  }
}
```

Slashing is implemented by just forwarding the `Slash` message to the `membership`
contract (which is set on instantiation - this is preasumed to be a mixer contract,
but technically it can be any contract implementing `tg4` and `Slashing` interfaces).
Obviously to be able to slash on the `membership` contracts, `tgrade-valset` has
to register itself as a slasher, so on the instantiation it would send the
`AddSlasher` message to the `membership` contract which has to succeed for the
whole instantiation to succeed. Therefore proper `slashing_preauths` has to be set
on `membership` contract.

`tgrade-valset` doesn't react to `AddSlasher` nor `RemoveSlasher` messages, as it
doesn't support multiple slashers. Only the admin of `tgrade-valset` can ever slash on
this contract (and he also always can do that).

Because only the `membership` contract is slashed by this implementation of `Slash`,
the `membership` contract itself is responsible for taking care of aligning
weight on validators and engagement contracts. However, as the rewards distribution
is not recalculated until the next epoch, the slashing would not affect the current
epoch.

## Init

```rust
pub struct InstantiateMsg {
    /// Address allowed to jail, meant to be a OC voting contract. If `None`, then jailing is
    /// impossible in this contract.
    pub admin: Option<String>,
    /// Address of a cw4 contract with the raw membership used to feed the validator set
    pub membership: String,
    /// Minimum weight needed by an address in `membership` to be considered for the validator set.
    /// 0-weight members are always filtered out.
    /// TODO: if we allow sub-1 scaling factors, determine if this is pre-/post- scaling
    /// (use weight for cw4, power for Tendermint)
    pub min_weight: u64,
    /// The maximum number of validators that can be included in the Tendermint validator set.
    /// If there are more validators than slots, we select the top N by membership weight
    /// descending. (In case of ties at the last slot, select by "first" Tendermint pubkey,
    /// lexicographically sorted).
    pub max_validators: u32,
    /// Number of seconds in one epoch. We update the Tendermint validator set only once per epoch.
    /// Epoch # is env.block.time/epoch_length (round down). The first block with a new epoch number
    /// will trigger a new validator calculation.
    pub epoch_length: u64,
    /// Total reward paid out at each epoch. This will be split among all validators during the last
    /// epoch.
    /// (epoch_reward.amount * 86_400 * 30 / epoch_length) is the amount of reward tokens to mint
    /// each month.
    /// Ensure this is sensible in relation to the total token supply.
    pub epoch_reward: Coin,

    /// Initial operators and validator keys registered.
    /// If you do not set this, the validators need to register themselves before
    /// making this privileged/calling the EndBlockers, so that we have a non-empty validator set
    pub initial_keys: Vec<OperatorInitInfo>,

    /// A scaling factor to multiply cw4-group weights to produce the Tendermint validator power
    /// (TODO: should we allow this to reduce weight? Like 1/1000?)
    pub scaling: Option<u32>,

    /// Percentage of total accumulated fees that is subtracted from tokens minted as rewards.
    /// 50% by default. To disable this feature just set it to 0 (which effectively means that fees
    /// don't affect the per-epoch reward).
    #[serde(default = "default_fee_percentage")]
    pub fee_percentage: Decimal,

    /// Flag determining if validators should be automatically unjailed after the jailing period;
    /// false by default.
    #[serde(default)]
    pub auto_unjail: bool,

    /// Validators who are caught double signing are jailed forever and their bonded tokens are
    /// slashed based on this value.
    #[serde(default = "default_double_sign_slash")]
    pub double_sign_slash_ratio: Decimal,

    /// Addresses where part of the reward for non-validators is sent for further distribution. These are
    /// required to handle the `Distribute {}` message (eg. tg4-engagement contract) which would
    /// distribute the funds sent with this message.
    ///
    /// The sum of ratios here has to be in the [0, 1] range. The remainder is sent to validators via the
    /// rewards contract.
    ///
    /// Note that the particular algorithm this contract uses calculates token rewards for distribution
    /// contracts by applying decimal division to the pool of reward tokens, and then passes the remainder
    /// to validators via the contract instantiated from `rewards_code_is`. This will cause edge cases where
    /// indivisible tokens end up with the validators. For example if the reward pool for an epoch is 1 token
    /// and there are two distribution contracts with 50% ratio each, that token will end up with the
    /// validators.
    pub distribution_contracts: UnvalidatedDistributionContracts,

    /// Code id of the contract which would be used to distribute the rewards of this token, assuming
    /// `tg4-engagement`. The contract will be initialized with the message:
    /// ```json
    /// {
    ///     "admin": "valset_addr",
    ///     "denom": "reward_denom",
    /// }
    /// ```
    ///
    /// This contract has to support all the `RewardsDistribution` messages
    pub rewards_code_id: u64,
}
```

## Messages

```rust
pub enum ExecuteMsg {
    /// Change the admin
    UpdateAdmin {
        admin: Option<String>,
    },
    /// Links info.sender (operator) to this Tendermint consensus key.
    /// The operator cannot re-register another key.
    /// No two operators may have the same consensus_key.
    RegisterValidatorKey {
        pubkey: Pubkey,
        /// Additional metadata assigned to this validator
        metadata: ValidatorMetadata,
    },
    UpdateMetadata(ValidatorMetadata),
    /// Jails validator. Can be executed only by the admin.
    Jail {
        /// Operator which should be jailed
        operator: String,
        /// Duration for how long validator is jailed, `None` for jailing forever
        duration: Option<Duration>,
    },
    /// Unjails validator. Admin can unjail anyone anytime, others can unjail only themselves and
    /// only if the jail period passed.
    Unjail {
        /// Address to unjail. Optional, as if not provided it is assumed to be the sender of the
        /// message (for convenience when unjailing self after the jail period).
        operator: Option<String>,
    },
    /// To be called by admin only. Slashes a given address (by forwarding slash to both rewards
    /// contract and engagement contract)
    Slash {
        addr: String,
        portion: Decimal,
    },
}

pub struct ValidatorMetadata {
    /// The validator's name (required)
    pub moniker: String,

    /// The optional identity signature (ex. UPort or Keybase)
    pub identity: Option<String>,

    /// The validator's (optional) website
    pub website: Option<String>,

    /// The validator's (optional) security contact email
    pub security_contact: Option<String>,

    /// The validator's (optional) details
    pub details: Option<String>,
}
```

## Queries

```rust
pub enum QueryMsg {
    /// Returns ConfigResponse - static contract data
    Config {},
    /// Returns EpochResponse - get info on current and next epochs
    Epoch {},

    /// Returns the validator key and associated metadata (if present) for the given operator.
    /// Returns ValidatorResponse
    Validator { operator: String },
    /// Paginate over all operators, using operator address as pagination.
    /// Returns ListValidatorsResponse
    ListValidators {
        start_after: Option<String>,
        limit: Option<u32>,
    },

    /// List the current validator set, sorted by power descending
    /// (no pagination - reasonable limit from max_validators)
    ListActiveValidators {},

    /// This will calculate who the new validators would be if
    /// we recalculated end block right now.
    /// Also returns ListActiveValidatorsResponse
    SimulateActiveValidators {},

    /// Returns a list of validator slashing events.
    /// Returns ListValidatorSlashingResponse
    ListValidatorSlashing { operator: String },
}
```
