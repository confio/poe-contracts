# Vesting Account as a contract

## Vesting Accounts

Many stakeholders will receive vesting accounts of tokens that will be released over 1-3 years. There exists an implementation for Cosmos SDK, but it doesn't seem to be a great fit. Some design requirements we have:
- Recipient should only have tax burden when they receive the unvested tokens (that they can sell to cover it). This is a similar issue like stock in private company, which is taxed but cannot be sold to cover taxes.
- If vesting tokens belong to a validator, they should be able to stake (and unstake) them while they are still vesting (even if they cannot transfer them)
- Fully vested tokens are guaranteed to be under the control of the originally defined recipient.
- This can be implemented as a CosmWasm contract
- (Optional) there may be a way to freeze payout if the recipient violates a contractual agreement (eg. a validator leaves the network after 1 month but gets all tokens). Note the Validator agreement has a clause where the validator agrees to run a node for a minimum period, if there is a breach of agreement then there are legal options available to claim the tokens back.

When discussing the legal / tax issues with Martin, we came to the conclusion that all unvested / unreleased tokens must be under the control of SOB to ensure the tax burden remains with SOB (which doesn't have an issue here) and not the ultimate recipient.

## Key Tax considerations
It is important when considering what can be viewed as a taxable event.

If the contract is devised with a fixed date (i.e. at the end of the vesting period) even if it requires the SOB multisig the tax authorities will consider this some form of obfuscation/tax avoidance and potentially ignore the release date. If there is discretion from the SOB board it can be argued that the tokens have little to no value as they may never be awarded, so from an income tax perspective you receive the tokens booked at nil or a symbolic Euro, and when you dispose of your tokens you then are subject to capital gains tax.

This then creates a further issue in whether people trust SOB to release the tokens. We also need to ensure the board composition has a quorum of members that do not directly benefit from the scheme (the board is about to be 5 members, 2 of which are Confio) and board members who are beneficiaries must abstain from resolutions to release tokens.

It is important that we get tax advice from the jurisdictions where employees live and where appropriate we may need different smart contracts for the relevant jurisdictions. Not all jurisdictions may accept the nil value based on the discretion of the SOB.

### Actors
- Operator - this is either the validator or an optional delegation to an "operational" employee from SOB, which can approve the payout of fully vested tokens to the final recipient. They cannot do anything else
- Oversight - this is a secure multi-sig from SOB, which can be used in extraordinary circumstances, to change the Operator, or to halt the release of future tokens in the case of misbehaviour.
- Recipient - this is the account that receives the tokens once they have been vested and released. This cannot be changed. Tokens not released for whatever reason will be effectively burned, so SOB cannot repurpose them.

### Setup
A vesting contract can be defined easily in the tgrade genesis file, or can be created any time afterwards in a running chain (also by other actors besides SOB). When creating a contract the three accounts above must be defined (Operator, Oversight, and Recipient). We must also define vesting tokens and schedule. Vesting tokens are the total number of tokens to be released, and the schedule defines when they are available.

We will keep this easy for now and only allow schedules that can be represented as a piecewise linear curve. That is, 0% until "start time", 100% after "end time" and a linear increase between the two. While the schedule is continuous, this doesn't mean the tokens will all be released every block, just the allowed limit increases.

### Releasing Tokens
The operator is responsible for releasing tokens. This employee should be handling a more or less routine job, like payroll. Once a month, the key can sign off on all vesting accounts to release all available tokens to the recipient account, providing a monthly income between start time and end time.

The human element allows easy customisation. For example, some contracts may define releasing the vested tokens in chunks every 6 months. Or a recipient may request to delay some payments until a later date (for example, "don't release any tokens until January 2023"). Releasing less than the allowed limits is not enforced on the blockchain, but handled by the employee in any agreement made by the two parties.

We store a total number of released tokens on the contract.

### Misbehavior
If the Operator loses their key or refuse to release the appropriate tokens for any reason, the Oversight key can replace the Operator key.

If the Recipient has violated any agreement that were the basis for the vesting schedule, the Oversight account can "freeze" those tokens. It can be a partial freeze, and it can be undone by a future actions by the Oversight account. We store a number of frozen tokens as well as the released tokens.

Note: There is a validator agreement in place which each validator signs in return for the receipt of the tokens.

### Staking Tokens
If the vesting Recipient is a validator, they will want to use the tokens for staking while they are vesting. To allow this, the Operator may perform two actions with the tokens besides releasing to the Recipient - Bond and Unbond. They will bond to the validator tg4-stake contract under the address of this contract. Which means that the vesting contract must be the official operator of the node (and also collect the engagement points for this validator).

Until the contract is fully vested, the Recipient must contact the Operator in order to initiate any bonding/unbonding. The bonded tokens are not stored under the contact, but counted as "vesting tokens". This may lead to the case where more tokens are available to release than are in the contract, and an unbonding may have to occur in order to release them (which is handled by communication between the Operator and Recipient)

### Hand Over
Most users are happy to pull out their last tokens into their normal account and then ignore the now-empty vesting account. However, validators will want to keep using this account and want full control after the vesting period is over. Here we define a manner of such a hand-off. The goal being that this Vesting Contract convert into a fully functional "proxy account" under the control of the Recipient, but also that any frozen tokens not be available to them.

Once the end_time of the contract has passed, either the Recipient or Oversight can invoke a HandOff. This will first burn the frozen tokens (if any) and set the frozen tokens count to 0. If the contract doesn't have enough tokens to burn (as they are staked), this will fail, and must be repeated once those tokens can be burnt. After burning the tokens, the Oversight and Operator keys are set to the Recipient key and the contract is marked as "liberated".

A "liberated" contract may perform any of the actions of a normal vesting contact, with Oversight set to a Recipient. Meaning the Recipient could set a new Operator key that can only bond/unbond or return tokens to the pre-defined Recipient account. It will also allow the new Oversight to modify the Recipient address (as we consider these all belonging to the same organization at this point).

Furthermore, it allows the Oversight to set a new Oversight address (key rotation) and it allows the Oversight address to execute any message on behalf of the contract address - voting, swapping in a Dex, etc. It will become quite similar to a standard "proxy contract", like cw1-whitelist.

Note that a "Hand Over" is a taxable event and should be booked as a transfer of tokens to Recipient. We should ensure this info is available in the event system so we can track it with reporting tools. This also means that we should not automatically hand over the account at the end of the vesting period, but only on request of the Recipient.

### Details: Calculating Tokens that can be Released
When calculating tokens that can be released, we use the following equations:
- Available tokens = Vested Tokens - Released Tokens - Frozen Tokens
- If t >= end_time, Vested Tokens = Balance(contract) + Released Tokens
  - this handles case where more tokens were sent to contract later, and just keeps the frozen tokens frozen
- If start_time >= t, Vested Tokens = 0
- If start_time < t < end_time, Vested Tokens = InitialBalance * (t - start_time) / (end_time - start_time)

Example:

12 months schedule, total 400.000 tokens.\
Month 2: Accidentally send 50.000 tokens to the contract, but they don't affect schedule.\
Month 3: 100.000 are released. (all that were vested from original 400.000)\
Month 5: freeze 200.000 for misbehaviour\
Month 6: No tokens can be released (200.000 - 100.000 - 200.000)\
Month 10: 25.000 tokens are released (out of 333.333 - 100.000 - 200.000 = 33.333)\
Month 12: All remaining tokens are released, that is Balance of 325.000 - 200.000 frozen = 125.000 (this is the 75.000 that finished vesting as well as the 50.000 that got sent but locked until end of schedule)\

## Init

...

## Messages

...
