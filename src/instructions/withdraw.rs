use pinocchio::{
    AccountView, Address,
    cpi::{Seed, Signer},
    error::ProgramError,
    ProgramResult,
};
use pinocchio_token::state::{Mint, TokenAccount};
use crate::batch::{batch, TokenOp};
use crate::constants::{CONFIG_SEED, LP_SEED, MINIMUM_LIQUIDITY};
use crate::errors::AmmError;
use crate::state::Config;

const DATA_LEN: usize = 32;

/// Accounts:
///  0. user           [signer]
///  1. mint_x         []
///  2. mint_y         []
///  3. mint_lp        [mut]
///  4. vault_x        [mut]
///  5. vault_y        [mut]
///  6. user_x         [mut]
///  7. user_y         [mut]
///  8. user_lp        [mut]
///  9. config         []
/// 10. token_program  []
pub struct WithdrawAccounts<'a> {
    pub user: &'a AccountView,
    pub mint_x: &'a AccountView,
    pub mint_y: &'a AccountView,
    pub mint_lp: &'a AccountView,
    pub vault_x: &'a AccountView,
    pub vault_y: &'a AccountView,
    pub user_x: &'a AccountView,
    pub user_y: &'a AccountView,
    pub user_lp: &'a AccountView,
    pub config: &'a AccountView,
    pub token_program: &'a AccountView,
}

impl<'a> TryFrom<&'a [AccountView]> for WithdrawAccounts<'a> {
    type Error = ProgramError;

    fn try_from(accounts: &'a [AccountView]) -> Result<Self, ProgramError> {
        let [
            user, mint_x, mint_y, mint_lp,
            vault_x, vault_y,
            user_x, user_y, user_lp,
            config, token_program,
            ..
        ] = accounts else {
            return Err(ProgramError::NotEnoughAccountKeys);
        };

        if !user.is_signer() {
            return Err(AmmError::NotSigner.into());
        }

        Ok(Self {
            user, mint_x, mint_y, mint_lp,
            vault_x, vault_y,
            user_x, user_y, user_lp,
            config, token_program,
        })
    }
}

pub struct WithdrawInstructionData {
    pub amount: u64,
    pub min_x: u64,
    pub min_y: u64,
    pub expiration: i64,
}

impl TryFrom<&[u8]> for WithdrawInstructionData {
    type Error = ProgramError;

    fn try_from(data: &[u8]) -> Result<Self, ProgramError> {
        if data.len() < DATA_LEN {
            return Err(ProgramError::InvalidInstructionData);
        }
        Ok(Self {
            amount: u64::from_le_bytes(
                data[0..8].try_into().map_err(|_| ProgramError::InvalidInstructionData)?,
            ),
            min_x: u64::from_le_bytes(
                data[8..16].try_into().map_err(|_| ProgramError::InvalidInstructionData)?,
            ),
            min_y: u64::from_le_bytes(
                data[16..24].try_into().map_err(|_| ProgramError::InvalidInstructionData)?,
            ),
            expiration: i64::from_le_bytes(
                data[24..32].try_into().map_err(|_| ProgramError::InvalidInstructionData)?,
            ),
        })
    }
}

pub struct Withdraw<'a> {
    pub accounts: WithdrawAccounts<'a>,
    pub instruction_data: WithdrawInstructionData,
}

impl<'a> TryFrom<(&[u8], &'a [AccountView])> for Withdraw<'a> {
    type Error = ProgramError;

    fn try_from((data, accounts): (&[u8], &'a [AccountView])) -> Result<Self, ProgramError> {
        crate::sol_log("Withdraw");

        let accounts = WithdrawAccounts::try_from(accounts)?;
        let instruction_data = WithdrawInstructionData::try_from(data)?;

        Ok(Self { accounts, instruction_data })
    }
}

impl<'a> Withdraw<'a> {
    pub const DISCRIMINATOR: &'a u8 = &2;

    pub fn process(&mut self) -> ProgramResult {
        let a = &self.accounts;

        let (seed_bytes, mint_x_bytes, mint_y_bytes, locked, config_bump) = {
            let config = Config::load(a.config)?;
            (
                config.seed().to_le_bytes(),
                *config.mint_x(),
                *config.mint_y(),
                config.locked(),
                config.bump(),
            )
        };

        let (expected_config, _) = Address::find_program_address(
            &[CONFIG_SEED, &seed_bytes, &mint_x_bytes, &mint_y_bytes],
            &crate::ID,
        );
        if expected_config.ne(a.config.address()) {
            return Err(AmmError::InvalidPda.into());
        }

        if a.mint_x.address().as_array() != &mint_x_bytes {
            return Err(AmmError::InvalidMint.into());
        }
        if a.mint_y.address().as_array() != &mint_y_bytes {
            return Err(AmmError::InvalidMint.into());
        }

        let (expected_lp, _) = Address::find_program_address(
            &[LP_SEED, a.config.address().as_ref()],
            &crate::ID,
        );
        if expected_lp.ne(a.mint_lp.address()) {
            return Err(AmmError::InvalidPda.into());
        }

        let (expected_vault_x, _) = Address::find_program_address(
            &[a.config.address().as_ref(), a.token_program.address().as_ref(), &mint_x_bytes],
            &pinocchio_associated_token_account::ID,
        );
        if expected_vault_x.ne(a.vault_x.address()) {
            return Err(AmmError::InvalidVaultAddress.into());
        }
        let (expected_vault_y, _) = Address::find_program_address(
            &[a.config.address().as_ref(), a.token_program.address().as_ref(), &mint_y_bytes],
            &pinocchio_associated_token_account::ID,
        );
        if expected_vault_y.ne(a.vault_y.address()) {
            return Err(AmmError::InvalidVaultAddress.into());
        }

        {
            let user_x_ta = TokenAccount::from_account_view(a.user_x)?;
            if user_x_ta.mint().as_array() != &mint_x_bytes {
                return Err(AmmError::InvalidMint.into());
            }
        }
        {
            let user_y_ta = TokenAccount::from_account_view(a.user_y)?;
            if user_y_ta.mint().as_array() != &mint_y_bytes {
                return Err(AmmError::InvalidMint.into());
            }
        }

        if locked {
            return Err(AmmError::PoolLocked.into());
        }

        #[cfg(target_os = "solana")]
        {
            use pinocchio::sysvars::Sysvar;
            let clock = pinocchio::sysvars::clock::Clock::get()?;
            if self.instruction_data.expiration <= clock.unix_timestamp {
                return Err(AmmError::OfferExpired.into());
            }
        }

        if self.instruction_data.amount == 0
            || self.instruction_data.min_x == 0
            || self.instruction_data.min_y == 0
        {
            return Err(AmmError::InvalidAmount.into());
        }

        let (vault_x_bal, vault_y_bal, lp_supply) = {
            let vx = TokenAccount::from_account_view(a.vault_x)?;
            let vy = TokenAccount::from_account_view(a.vault_y)?;
            let lp = Mint::from_account_view(a.mint_lp)?;
            (vx.amount(), vy.amount(), lp.supply())
        };

        let adjusted_supply = (lp_supply as u128)
            .checked_add(MINIMUM_LIQUIDITY as u128)
            .ok_or(AmmError::Overflow)?;

        let x_out = u64::try_from(
            (vault_x_bal as u128)
                .checked_mul(self.instruction_data.amount as u128)
                .ok_or(AmmError::Overflow)?
                .checked_div(adjusted_supply)
                .ok_or(AmmError::Overflow)?,
        )
        .map_err(|_| AmmError::Overflow)?;

        let y_out = u64::try_from(
            (vault_y_bal as u128)
                .checked_mul(self.instruction_data.amount as u128)
                .ok_or(AmmError::Overflow)?
                .checked_div(adjusted_supply)
                .ok_or(AmmError::Overflow)?,
        )
        .map_err(|_| AmmError::Overflow)?;

        if x_out < self.instruction_data.min_x || y_out < self.instruction_data.min_y {
            return Err(AmmError::SlippageExceeded.into());
        }

        let bump_ref = [config_bump];
        let config_seeds = [
            Seed::from(CONFIG_SEED),
            Seed::from(seed_bytes.as_ref()),
            Seed::from(mint_x_bytes.as_ref()),
            Seed::from(mint_y_bytes.as_ref()),
            Seed::from(&bump_ref),
        ];
        let signers = [Signer::from(&config_seeds)];

        batch::<3, 9>(
            [
                TokenOp::transfer(a.vault_x, a.user_x, a.config, x_out),
                TokenOp::transfer(a.vault_y, a.user_y, a.config, y_out),
                TokenOp::burn(a.user_lp, a.mint_lp, a.user, self.instruction_data.amount),
            ],
            a.token_program,
            &signers,
        )?;

        Ok(())
    }
}
