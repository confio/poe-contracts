# Migrating

This guide lists API changes between releases of *PoE* contracts.

## 0.6.0-beta1 -> 0.6.0-beta2

### tg4-engagement

Messages changes:

* `distribute_funds` message renamed to `distribute_rewards`
* `withdraw_funds` message renamed to `withdraw_rewards`
* `total_weight` query renamed to `total_points`
* `list_members_by_weight` query renamed to `list_members_by_points`
* `withdrawable_funds` query renamed to `withdrawable_rewards`
* `distributed_funds` query renamed to `distributed_rewards`
* `undistributed_funds` query renamed to `undistributed_rewards`

State changes:

* `points_per_weight` field on `distribution` renamed to `shares_per_point`
* `points_leftover` field on `distribution` renamed to `shares_leftover`
* `points_correction` field on `withdraw_adjustment` map items renamed to `shares_correction`
* `withdrawn_funds` field on `withdraw_adjustment` map items renamed to `withdrawn_rewards`

### tg4-mixer

Messages changes:

* `total_weight` query renamed to `total_points`
* `list_members_by_weight` query renamed to `list_members_by_points`
* `reward_function` query renamed to `mixer_function`
* `reward` field on response to `mixer_function` (`reward_function` previously)
  renamed to `points`

### tg4-stake

Messages changes:

* `tokens_per_weight` field on instantiate message renamed to `tokens_per_point`
* `total_weight` query renamed to `total_points`
* `list_members_by_weight` query renamed to `list_members_by_points`
* `weight` field on response to `total_points` (`total_weight` previously)
  renamed to `points`

State changes:

* `tokens_per_weight` field on `config` item renamed to `tokens_per_point`

### tgrade-community-pool

Messages changes:

* `distribute_funds` message renamed to `distribute_rewards`

### tgrade-valset

Messages changes:

* `min_weight` field on instantiate message renamed to `min_points`

State changes:

* `min_weight` field on `config` item renamed to `min_points`

