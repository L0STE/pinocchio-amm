use pinocchio::error::ProgramError;
use crate::constants::MAX_FEE_BPS;
use crate::errors::AmmError;

pub struct SwapResult {
    pub deposit: u64,
    pub withdraw: u64,
    pub fee: u64,
}

/// Constant product swap: output = reserve_out * amount_net / (reserve_in + amount_net)
pub fn swap(
    reserve_in: u64,
    reserve_out: u64,
    amount_in: u64,
    fee_bps: u16,
    min_out: u64,
) -> Result<SwapResult, ProgramError> {
    if reserve_in == 0 || reserve_out == 0 {
        return Err(AmmError::ZeroBalance.into());
    }
    if amount_in == 0 {
        return Err(AmmError::InvalidAmount.into());
    }

    let amount_in_128 = amount_in as u128;
    let reserve_in_128 = reserve_in as u128;
    let reserve_out_128 = reserve_out as u128;

    let fee_amount = amount_in_128
        .checked_mul(fee_bps as u128)
        .ok_or(AmmError::Overflow)?
        / MAX_FEE_BPS as u128;

    let amount_net = amount_in_128
        .checked_sub(fee_amount)
        .ok_or(AmmError::Underflow)?;

    let numerator = reserve_out_128
        .checked_mul(amount_net)
        .ok_or(AmmError::Overflow)?;

    let denominator = reserve_in_128
        .checked_add(amount_net)
        .ok_or(AmmError::Overflow)?;

    let output = numerator / denominator;

    let fee_u64 = u64::try_from(fee_amount).map_err(|_| AmmError::Overflow)?;
    let withdraw_u64 = u64::try_from(output).map_err(|_| AmmError::Overflow)?;

    if withdraw_u64 == 0 {
        return Err(AmmError::InvalidAmount.into());
    }

    if withdraw_u64 < min_out {
        return Err(AmmError::SlippageExceeded.into());
    }

    Ok(SwapResult {
        deposit: amount_in,
        withdraw: withdraw_u64,
        fee: fee_u64,
    })
}

/// Integer sqrt via Newton's method.
pub fn integer_sqrt(n: u128) -> u128 {
    if n == 0 {
        return 0;
    }
    let mut x = n;
    let mut y = (x + 1) / 2;
    while y < x {
        x = y;
        y = (x + n / x) / 2;
    }
    x
}
