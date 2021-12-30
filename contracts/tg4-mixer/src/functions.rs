use integer_sqrt::IntegerSquareRoot;
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::{Decimal, MathematicalOps};
use rust_decimal_macros::dec;

use cosmwasm_std::{Decimal as StdDecimal, Fraction, Uint64};

use crate::error::ContractError;

pub fn std_to_decimal(std_decimal: StdDecimal) -> Decimal {
    Decimal::from_i128_with_scale(std_decimal.numerator().u128() as i128, 18) // FIXME: StdDecimal::DECIMAL_PLACES is private
}

/// This defines the functions we can use for proof of engagement rewards.
pub trait PoEFunction {
    /// Returns the rewards based on the amount of stake and engagement points.
    /// `f(x)` from the README.
    fn rewards(&self, stake: u64, engagement: u64) -> Result<u64, ContractError>;
}

/// This takes a geometric mean of stake and engagement points using integer math
#[derive(Default)]
pub struct GeometricMean {}

impl GeometricMean {
    pub fn new() -> Self {
        Self::default()
    }
}

impl PoEFunction for GeometricMean {
    fn rewards(&self, stake: u64, engagement: u64) -> Result<u64, ContractError> {
        let mult = (stake as u128) * (engagement as u128);
        Ok(mult.integer_sqrt() as u64)
    }
}

/// Sigmoid function. `f(x) = 1 / (1 + e^-x)`.
/// Fitting the sigmoid-like function from the PoE whitepaper:
/// `f(x) = r_max * (2 / (1 + e ^(-s * x^p) - 1)`
pub struct Sigmoid {
    pub max_rewards: Decimal,
    pub p: Decimal,
    pub s: Decimal,
    zero: Decimal,
    one: Decimal,
    two: Decimal,
}

impl Sigmoid {
    pub fn new(max_rewards: Uint64, p: StdDecimal, s: StdDecimal) -> Result<Self, ContractError> {
        Self::validate(&max_rewards, &p, &s)?;
        Ok(Self {
            max_rewards: Decimal::new(max_rewards.u64() as i64, 0),
            p: std_to_decimal(p),
            s: std_to_decimal(s),
            zero: dec!(0),
            one: dec!(1),
            two: dec!(2),
        })
    }

    fn validate(max_rewards: &Uint64, p: &StdDecimal, s: &StdDecimal) -> Result<(), ContractError> {
        // validate `max_rewards`
        if max_rewards.u64() > i64::MAX as u64 {
            return Err(ContractError::ParameterRange(
                "max_rewards",
                max_rewards.to_string(),
            ));
        }

        // validate `p`
        if !(StdDecimal::zero()..=StdDecimal::one()).contains(p) {
            return Err(ContractError::ParameterRange("p", p.to_string()));
        }

        // validate `s`
        if !(StdDecimal::zero()..=StdDecimal::one()).contains(s) {
            return Err(ContractError::ParameterRange("s", s.to_string()));
        }
        Ok(())
    }
}

impl PoEFunction for Sigmoid {
    fn rewards(&self, stake: u64, engagement: u64) -> Result<u64, ContractError> {
        // Cast to i64 because of rust_decimal::Decimal underlying impl
        let left = Decimal::new(stake as i64, 0);
        let right = Decimal::new(engagement as i64, 0);

        // Rejects u64 values larger than 2^63, which become negative in Decimal
        if left.is_sign_negative() || right.is_sign_negative() {
            return Err(ContractError::WeightOverflow {});
        }

        // This is the implementation of the PoE whitepaper, Appendix A,
        // "root of engagement" sigmoid-like function, using fixed point math.
        // `reward = r_max * (2 / (1 + e^(-s * (stake * engagement)^p) ) - 1)`
        // We distribute the power over the factors here, just to extend the range of the function.
        // Given that `s` is always positive, we also replace the underflowed exponential case
        // with zero (also to extend the range).
        let reward = self.max_rewards
            * (self.two
                / (self.one
                    + (-self.s
                        * left
                            .checked_powd(self.p)
                            .ok_or(ContractError::ComputationOverflow("powd"))?
                            .checked_mul(
                                right
                                    .checked_powd(self.p)
                                    .ok_or(ContractError::ComputationOverflow("powd"))?,
                            )
                            .ok_or(ContractError::ComputationOverflow("mul"))?)
                    .checked_exp()
                    .unwrap_or(self.zero))
                - self.one);

        reward.to_u64().ok_or(ContractError::RewardOverflow {})
    }
}

/// Sigmoid function. `f(x) = 1 / (1 + e^-x)`.
/// Fitting the sigmoid-like to a 1/2 (sqrt) exponent.
/// `f(x) = r_max * (2 / (1 + e ^(-s * sqrt(x)) - 1)`
pub struct SigmoidSqrt {
    pub max_rewards: Decimal,
    pub s: Decimal,
    geometric: GeometricMean,
    zero: Decimal,
    one: Decimal,
    two: Decimal,
}

impl SigmoidSqrt {
    pub fn new(max_rewards: Uint64, s: StdDecimal) -> Result<Self, ContractError> {
        Self::validate(&max_rewards, &s)?;
        Ok(Self {
            max_rewards: Decimal::new(max_rewards.u64() as i64, 0),
            s: std_to_decimal(s),
            geometric: GeometricMean::new(),
            zero: dec!(0),
            one: dec!(1),
            two: dec!(2),
        })
    }

    fn validate(max_rewards: &Uint64, s: &StdDecimal) -> Result<(), ContractError> {
        // validate `max_rewards`
        if max_rewards.u64() > i64::MAX as u64 {
            return Err(ContractError::ParameterRange(
                "max_rewards",
                max_rewards.to_string(),
            ));
        }

        // validate `s`
        if !(StdDecimal::zero()..=StdDecimal::one()).contains(s) {
            return Err(ContractError::ParameterRange("s", s.to_string()));
        }
        Ok(())
    }
}

impl PoEFunction for SigmoidSqrt {
    fn rewards(&self, stake: u64, engagement: u64) -> Result<u64, ContractError> {
        // `reward = r_max * (2 / (1 + e^(-s * sqrt(stake * engagement)) ) - 1)`
        let geometric_mean = self.geometric.rewards(stake, engagement).unwrap();
        let geometric_mean = Decimal::new(geometric_mean as i64, 0);
        if geometric_mean.is_sign_negative() {
            return Err(ContractError::WeightOverflow {});
        }

        // Given that `s` is always positive, we replace the underflowed exponential case
        // with zero (also to extend the range).
        let reward = self.max_rewards
            * (self.two
                / (self.one
                    + (-self.s * geometric_mean)
                        .checked_exp()
                        .unwrap_or(self.zero))
                - self.one);

        reward.to_u64().ok_or(ContractError::RewardOverflow {})
    }
}

/// Algebraic sigmoid. `f(x) = x / sqrt(1 + x^2)`.
/// Fitting the sigmoid-like function from the PoE whitepaper.
/// `p` and `s` are just equivalent to the `Sigmoid` parameters.
/// `a` is an adjustment / fitting parameter (`1 <= a < 5`), to better match the
/// two curves differing slopes.
pub struct AlgebraicSigmoid {
    pub max_rewards: Decimal,
    pub a: Decimal,
    pub p: Decimal,
    pub s: Decimal,
}

impl AlgebraicSigmoid {
    pub fn new(
        max_rewards: Uint64,
        a: StdDecimal,
        p: StdDecimal,
        s: StdDecimal,
    ) -> Result<Self, ContractError> {
        Self::validate(&max_rewards, &a, &p, &s)?;
        Ok(Self {
            max_rewards: Decimal::new(max_rewards.u64() as i64, 0),
            a: std_to_decimal(a),
            p: std_to_decimal(p),
            s: std_to_decimal(s),
        })
    }

    fn validate(
        max_rewards: &Uint64,
        a: &StdDecimal,
        p: &StdDecimal,
        s: &StdDecimal,
    ) -> Result<(), ContractError> {
        // validate `max_rewards`
        if max_rewards.u64() > i64::MAX as u64 {
            return Err(ContractError::ParameterRange(
                "max_rewards",
                max_rewards.to_string(),
            ));
        }

        // validate `a`
        if !(StdDecimal::zero()..=StdDecimal::from_ratio(5u8, 1u8)).contains(a) {
            return Err(ContractError::ParameterRange("a", a.to_string()));
        }

        // validate `p`
        if !(StdDecimal::zero()..=StdDecimal::one()).contains(p) {
            return Err(ContractError::ParameterRange("p", p.to_string()));
        }

        // validate `s`
        if !(StdDecimal::zero()..=StdDecimal::one()).contains(s) {
            return Err(ContractError::ParameterRange("s", s.to_string()));
        }
        Ok(())
    }
}

impl PoEFunction for AlgebraicSigmoid {
    fn rewards(&self, stake: u64, engagement: u64) -> Result<u64, ContractError> {
        // Cast to i64 because of rust_decimal::Decimal underlying impl
        let left = Decimal::new(stake as i64, 0);
        let right = Decimal::new(engagement as i64, 0);

        // Rejects u64 values larger than 2^63, which become negative in Decimal
        if left.is_sign_negative() || right.is_sign_negative() {
            return Err(ContractError::WeightOverflow {});
        }

        // x = s * (reward * engagement)^p
        // We distribute the power over the factors here, just to extend the range of the function.
        let x = self.s
            * left
                .checked_powd(self.p)
                .ok_or(ContractError::ComputationOverflow("powd"))?
                .checked_mul(
                    right
                        .checked_powd(self.p)
                        .ok_or(ContractError::ComputationOverflow("powd"))?,
                )
                .ok_or(ContractError::ComputationOverflow("mul"))?;

        // reward = r_max * x / sqrt(a + x^2)
        let reward = self.max_rewards * x
            / (self.a
                + x.checked_powu(2)
                    .ok_or(ContractError::ComputationOverflow("powu"))?)
            .sqrt()
            .ok_or(ContractError::ComputationOverflow("sqrt"))?;

        reward.to_u64().ok_or(ContractError::RewardOverflow {})
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mixer_geometric_works() {
        let geometric = GeometricMean::new();

        // either 0 -> 0
        assert_eq!(geometric.rewards(0, 123456).unwrap(), 0);
        assert_eq!(geometric.rewards(7777, 0).unwrap(), 0);

        // basic math checks (no rounding)
        assert_eq!(geometric.rewards(4, 9).unwrap(), 6);

        // rounding down (sqrt(240) = 15.49...
        assert_eq!(geometric.rewards(12, 20).unwrap(), 15);

        // not overflow checks
        let very_big = u64::MAX;
        assert_eq!(geometric.rewards(very_big, very_big).unwrap(), very_big);
    }

    #[test]
    fn mixer_sigmoid_works() {
        let sigmoid = Sigmoid::new(
            Uint64::new(1000),
            StdDecimal::from_ratio(68u128, 100u128),
            StdDecimal::from_ratio(3u128, 100000u128),
        )
        .unwrap();

        // either 0 -> 0
        assert_eq!(sigmoid.rewards(0, 123456).unwrap(), 0);
        assert_eq!(sigmoid.rewards(7777, 0).unwrap(), 0);

        // Basic math checks (no rounding)
        // Values from PoE paper, Appendix A, "root of engagement" curve
        assert_eq!(sigmoid.rewards(5, 1000).unwrap(), 4);
        assert_eq!(sigmoid.rewards(5, 100000).unwrap(), 112);
        assert_eq!(sigmoid.rewards(1000, 1000).unwrap(), 178);
        assert_eq!(sigmoid.rewards(1000, 100000).unwrap(), 999);
        assert_eq!(sigmoid.rewards(100000, 100000).unwrap(), 1000);

        // Rounding down (697.8821566)
        assert_eq!(sigmoid.rewards(100, 100000).unwrap(), 697);

        // Overflow checks
        let err = sigmoid.rewards(u64::MAX, u64::MAX).unwrap_err();
        assert_eq!(err, ContractError::WeightOverflow {});

        // Very big, but positive in the i64 range
        let very_big = i64::MAX as u64;
        let err = sigmoid.rewards(very_big, very_big).unwrap_err();
        assert_eq!(err, ContractError::ComputationOverflow("powd"));

        // Precise limit
        let very_big = 32_313_447;
        assert_eq!(sigmoid.rewards(very_big, very_big).unwrap(), 1000);
        let err = sigmoid.rewards(very_big, very_big + 1).unwrap_err();
        assert_eq!(err, ContractError::ComputationOverflow("powd"));
    }

    #[test]
    fn mixer_sigmoid_half_works() {
        let sigmoid = Sigmoid::new(
            Uint64::new(1000),
            StdDecimal::from_ratio(5u128, 10u128),
            StdDecimal::from_ratio(3u128, 10000u128),
        )
        .unwrap();

        // either 0 -> 0
        assert_eq!(sigmoid.rewards(0, 123456).unwrap(), 0);
        assert_eq!(sigmoid.rewards(7777, 0).unwrap(), 0);

        // Basic math checks (no rounding)
        assert_eq!(sigmoid.rewards(5, 1000).unwrap(), 10);
        assert_eq!(sigmoid.rewards(5, 100000).unwrap(), 105);
        assert_eq!(sigmoid.rewards(1000, 1000).unwrap(), 148);
        assert_eq!(sigmoid.rewards(1000, 100000).unwrap(), 905);
        assert_eq!(sigmoid.rewards(100000, 100000).unwrap(), 1000);

        // Overflow checks
        let err = sigmoid.rewards(u64::MAX, u64::MAX).unwrap_err();
        assert_eq!(err, ContractError::WeightOverflow {});

        // Very big, but positive in the i64 range
        let very_big = i64::MAX as u64;
        let err = sigmoid.rewards(very_big, very_big).unwrap_err();
        assert_eq!(err, ContractError::ComputationOverflow("powd"));

        // Precise limit
        let very_big = 16_321_545_412;
        assert_eq!(sigmoid.rewards(very_big, very_big).unwrap(), 1000);
        let err = sigmoid.rewards(very_big, very_big + 1).unwrap_err();
        assert_eq!(err, ContractError::ComputationOverflow("powd"));
    }

    #[test]
    fn mixer_sigmoid_sqrt_works() {
        let sigmoid =
            SigmoidSqrt::new(Uint64::new(1000), StdDecimal::from_ratio(3u128, 10000u128)).unwrap();

        // either 0 -> 0
        assert_eq!(sigmoid.rewards(0, 123456).unwrap(), 0);
        assert_eq!(sigmoid.rewards(7777, 0).unwrap(), 0);

        // Basic math checks (no rounding)
        assert_eq!(sigmoid.rewards(5, 1000).unwrap(), 10);
        assert_eq!(sigmoid.rewards(5, 100000).unwrap(), 105);
        assert_eq!(sigmoid.rewards(1000, 1000).unwrap(), 148);
        assert_eq!(sigmoid.rewards(1000, 100000).unwrap(), 905);
        assert_eq!(sigmoid.rewards(100000, 100000).unwrap(), 1000);

        // Overflow checks
        let err = sigmoid.rewards(u64::MAX, u64::MAX).unwrap_err();
        assert_eq!(err, ContractError::WeightOverflow {});

        // Precise limit
        // Very big, but positive in the i64 range
        let very_big = i64::MAX as u64;
        assert_eq!(sigmoid.rewards(very_big, very_big).unwrap(), 1000);
        let err = sigmoid.rewards(very_big + 1, very_big + 1).unwrap_err();
        assert_eq!(err, ContractError::WeightOverflow {});
    }

    #[test]
    fn mixer_algebraic_sigmoid_works() {
        let algebraic_sigmoid = AlgebraicSigmoid::new(
            Uint64::new(1000),
            StdDecimal::from_ratio(371872u128, 100000u128),
            StdDecimal::from_ratio(68u128, 100u128),
            StdDecimal::from_ratio(3u128, 100000u128),
        )
        .unwrap();

        // either 0 -> 0
        assert_eq!(algebraic_sigmoid.rewards(0, 123456).unwrap(), 0);
        assert_eq!(algebraic_sigmoid.rewards(7777, 0).unwrap(), 0);

        // Basic math checks (no rounding)
        // Values from PoE paper, Appendix A, "root of engagement" curve
        assert_eq!(algebraic_sigmoid.rewards(5, 1000).unwrap(), 5);
        assert_eq!(algebraic_sigmoid.rewards(5, 100000).unwrap(), 115);
        assert_eq!(algebraic_sigmoid.rewards(1000, 1000).unwrap(), 183);
        assert_eq!(algebraic_sigmoid.rewards(1000, 100000).unwrap(), 973);
        assert_eq!(algebraic_sigmoid.rewards(100000, 100000).unwrap(), 999);

        // Overflow checks
        let err = algebraic_sigmoid.rewards(u64::MAX, u64::MAX).unwrap_err();
        assert_eq!(err, ContractError::WeightOverflow {});

        // Very big, but positive in the i64 range
        let very_big = i64::MAX as u64;
        let err = algebraic_sigmoid.rewards(very_big, very_big).unwrap_err();
        assert_eq!(err, ContractError::ComputationOverflow("powd"));

        // Precise limit
        let very_big = 32_313_447;
        assert_eq!(algebraic_sigmoid.rewards(very_big, very_big).unwrap(), 999);
        let err = algebraic_sigmoid
            .rewards(very_big, very_big + 1)
            .unwrap_err();
        assert_eq!(err, ContractError::ComputationOverflow("powd"));
    }
}
