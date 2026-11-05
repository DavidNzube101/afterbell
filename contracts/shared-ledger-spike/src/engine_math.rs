//! Pure fixed-point accounting helpers used by the A1 perpetuals engine.
//! Amounts are raw USDC units; prices use an arbitrary fixed-point scale.

pub const BPS: u128 = 10_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MathError {
    ZeroPrice,
    Overflow,
    InvalidLeverage,
}

pub fn ceil_div(value: u128, divisor: u128) -> Result<u128, MathError> {
    if divisor == 0 {
        return Err(MathError::ZeroPrice);
    }
    let quotient = value / divisor;
    quotient
        .checked_add(u128::from(value % divisor != 0))
        .ok_or(MathError::Overflow)
}

pub fn required_margin(notional: u128, leverage_bps: u32) -> Result<u128, MathError> {
    if leverage_bps < BPS as u32 {
        return Err(MathError::InvalidLeverage);
    }
    ceil_div(
        notional.checked_mul(BPS).ok_or(MathError::Overflow)?,
        u128::from(leverage_bps),
    )
}

/// Linear PnL, rounded toward zero. Longs gain when price rises; shorts gain
/// when it falls. The return is signed raw USDC units.
pub fn linear_pnl(
    notional: u128,
    entry_price: u128,
    mark_price: u128,
    is_long: bool,
) -> Result<i128, MathError> {
    if entry_price == 0 {
        return Err(MathError::ZeroPrice);
    }
    let delta = mark_price.abs_diff(entry_price);
    let magnitude = notional.checked_mul(delta).ok_or(MathError::Overflow)? / entry_price;
    let magnitude = i128::try_from(magnitude).map_err(|_| MathError::Overflow)?;
    let favorable = (mark_price >= entry_price) == is_long;
    Ok(if favorable { magnitude } else { -magnitude })
}

pub fn fee_ceil(notional: u128, fee_bps: u16) -> Result<u128, MathError> {
    ceil_div(
        notional
            .checked_mul(u128::from(fee_bps))
            .ok_or(MathError::Overflow)?,
        BPS,
    )
}

pub fn maintenance_margin(notional: u128, maintenance_bps: u16) -> Result<u128, MathError> {
    fee_ceil(notional, maintenance_bps)
}

/// A losing position is liquidatable when equity is strictly below its
/// maintenance requirement. Negative equity is represented as zero.
pub fn is_liquidatable(
    margin: u128,
    pnl: i128,
    current_notional: u128,
    maintenance_bps: u16,
) -> Result<bool, MathError> {
    let equity = if pnl < 0 {
        margin.saturating_sub(pnl.unsigned_abs())
    } else {
        margin.checked_add(pnl as u128).ok_or(MathError::Overflow)?
    };
    Ok(equity < maintenance_margin(current_notional, maintenance_bps)?)
}

/// Cumulative funding-index increment, in price units per base unit. A
/// caller supplies skew notional and total vault assets so the rate scales
/// with imbalance and is bounded by `max_rate_bps_per_hour`.
pub fn funding_rate_bps_per_hour(
    skew_notional: u128,
    vault_assets: u128,
    base_rate_bps: u16,
    max_rate_bps: u16,
) -> u16 {
    if vault_assets == 0 {
        return max_rate_bps;
    }
    let variable = skew_notional
        .saturating_mul(u128::from(max_rate_bps.saturating_sub(base_rate_bps)))
        / vault_assets;
    u128::from(base_rate_bps)
        .saturating_add(variable)
        .min(u128::from(max_rate_bps)) as u16
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn required_margin_rounds_up_and_rejects_leverage_below_one() {
        assert_eq!(required_margin(1_000_001, 30_000), Ok(333_334));
        assert_eq!(required_margin(1, 9_999), Err(MathError::InvalidLeverage));
    }

    #[test]
    fn linear_pnl_handles_both_sides_and_rounds_toward_zero() {
        assert_eq!(linear_pnl(1_000_000, 100, 110, true), Ok(100_000));
        assert_eq!(linear_pnl(1_000_000, 100, 90, true), Ok(-100_000));
        assert_eq!(linear_pnl(1_000_000, 100, 90, false), Ok(100_000));
        assert_eq!(linear_pnl(1, 3, 4, true), Ok(0));
    }

    #[test]
    fn pnl_and_fee_reject_overflow_or_zero_denominators() {
        assert_eq!(linear_pnl(1, 0, 1, true), Err(MathError::ZeroPrice));
        assert_eq!(linear_pnl(u128::MAX, 1, 2, true), Err(MathError::Overflow));
        assert_eq!(fee_ceil(u128::MAX, 100), Err(MathError::Overflow));
    }

    #[test]
    fn liquidations_compare_equity_to_maintenance_margin() {
        assert_eq!(is_liquidatable(100, -94, 100, 500), Ok(false));
        assert_eq!(is_liquidatable(100, -96, 100, 500), Ok(true));
        assert_eq!(is_liquidatable(10, -50, 1_000, 500), Ok(true));
    }

    #[test]
    fn funding_rate_is_skew_proportional_and_capped() {
        assert_eq!(funding_rate_bps_per_hour(0, 1_000, 1, 10), 1);
        assert_eq!(funding_rate_bps_per_hour(1, 0, 1, 10), 10);
        assert_eq!(funding_rate_bps_per_hour(500, 1_000, 1, 10), 5);
        assert_eq!(funding_rate_bps_per_hour(5_000, 1_000, 1, 10), 10);
    }
}
