use pinocchio::{
    AccountView, Address,
    cpi::{Seed, Signer},
    error::ProgramError,
    ProgramResult,
};
use pinocchio_token::state::TokenAccount;

use crate::batch::{batch, TokenOp};
use crate::constants::CONFIG_SEED;
use crate::curve;
use crate::errors::AmmError;
use crate::state::Config;

const DATA_LEN: usize = 24;

/// Accounts:
///  0. user           [signer]
///  1. mint_from      []
///  2. mint_to        []
///  3. user_from      [mut]
///  4. user_to        [mut]
///  5. vault_from     [mut]
///  6. vault_to       [mut]
///  7. config         []
///  8. token_program  []
pub struct SwapAccounts<'a> {
    pub user: &'a AccountView,
    pub mint_from: &'a AccountView,
    pub mint_to: &'a AccountView,
    pub user_from: &'a AccountView,
    pub user_to: &'a AccountView,
    pub vault_from: &'a AccountView,
    pub vault_to: &'a AccountView,
    pub config: &'a AccountView,
    pub token_program: &'a AccountView,
}

impl<'a> TryFrom<&'a [AccountView]> for SwapAccounts<'a> {
    type Error = ProgramError;

    fn try_from(accounts: &'a [AccountView]) -> Result<Self, ProgramError> {
        let [
            user, mint_from, mint_to,
            user_from, user_to,
            vault_from, vault_to,
            config, token_program,
            ..
        ] = accounts else {
            return Err(ProgramError::NotEnoughAccountKeys);
        };

        if !user.is_signer() {
            return Err(AmmError::NotSigner.into());
        }

        Ok(Self {
            user, mint_from, mint_to,
            user_from, user_to,
            vault_from, vault_to,
            config, token_program,
        })
    }
}

pub struct SwapInstructionData {
    pub amount: u64,
    pub min: u64,
    pub expiration: i64,
}

impl TryFrom<&[u8]> for SwapInstructionData {
    type Error = ProgramError;

    fn try_from(data: &[u8]) -> Result<Self, ProgramError> {
        if data.len() < DATA_LEN {
            return Err(ProgramError::InvalidInstructionData);
        }
        Ok(Self {
            amount: u64::from_le_bytes(
                data[0..8].try_into().map_err(|_| ProgramError::InvalidInstructionData)?,
            ),
            min: u64::from_le_bytes(
                data[8..16].try_into().map_err(|_| ProgramError::InvalidInstructionData)?,
            ),
            expiration: i64::from_le_bytes(
                data[16..24].try_into().map_err(|_| ProgramError::InvalidInstructionData)?,
            ),
        })
    }
}

pub struct Swap<'a> {
    pub accounts: SwapAccounts<'a>,
    pub instruction_data: SwapInstructionData,
}

impl<'a> TryFrom<(&[u8], &'a [AccountView])> for Swap<'a> {
    type Error = ProgramError;

    fn try_from((data, accounts): (&[u8], &'a [AccountView])) -> Result<Self, ProgramError> {
        crate::sol_log("Swap");

        let accounts = SwapAccounts::try_from(accounts)?;
        let instruction_data = SwapInstructionData::try_from(data)?;

        Ok(Self { accounts, instruction_data })
    }
}

impl<'a> Swap<'a> {
    pub const DISCRIMINATOR: &'a u8 = &3;

    pub fn process(&mut self) -> ProgramResult {
        let a = &self.accounts;

        let (seed_bytes, mint_x_bytes, mint_y_bytes, fee, locked, config_bump) = {
            let config = Config::load(a.config)?;
            (
                config.seed().to_le_bytes(),
                *config.mint_x(),
                *config.mint_y(),
                config.fee(),
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

        let (expected_vault_from, _) = Address::find_program_address(
            &[a.config.address().as_ref(), a.token_program.address().as_ref(), a.mint_from.address().as_ref()],
            &pinocchio_associated_token_account::ID,
        );
        if expected_vault_from.ne(a.vault_from.address()) {
            return Err(AmmError::InvalidVaultAddress.into());
        }
        let (expected_vault_to, _) = Address::find_program_address(
            &[a.config.address().as_ref(), a.token_program.address().as_ref(), a.mint_to.address().as_ref()],
            &pinocchio_associated_token_account::ID,
        );
        if expected_vault_to.ne(a.vault_to.address()) {
            return Err(AmmError::InvalidVaultAddress.into());
        }

        {
            let user_to_ta = TokenAccount::from_account_view(a.user_to)?;
            if user_to_ta.mint() != a.mint_to.address() {
                return Err(AmmError::InvalidMint.into());
            }
        }

        let mint_from_addr = a.mint_from.address().as_array();
        let mint_to_addr = a.mint_to.address().as_array();
        if !((mint_from_addr == &mint_x_bytes && mint_to_addr == &mint_y_bytes)
            || (mint_from_addr == &mint_y_bytes && mint_to_addr == &mint_x_bytes))
        {
            return Err(AmmError::InvalidMint.into());
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

        if self.instruction_data.amount == 0 || self.instruction_data.min == 0 {
            return Err(AmmError::InvalidAmount.into());
        }

        let (reserve_in, reserve_out) = {
            let vault_from_account = TokenAccount::from_account_view(a.vault_from)?;
            let vault_to_account = TokenAccount::from_account_view(a.vault_to)?;
            (vault_from_account.amount(), vault_to_account.amount())
        };

        let result = curve::swap(
            reserve_in, reserve_out,
            self.instruction_data.amount, fee, self.instruction_data.min,
        )?;

        let bump = [config_bump];
        let config_seeds = [
            Seed::from(CONFIG_SEED),
            Seed::from(seed_bytes.as_ref()),
            Seed::from(mint_x_bytes.as_ref()),
            Seed::from(mint_y_bytes.as_ref()),
            Seed::from(&bump),
        ];
        let signers = [Signer::from(&config_seeds)];

        batch::<2, 6>(
            [
                TokenOp::transfer(a.user_from, a.vault_from, a.user, result.deposit),
                TokenOp::transfer(a.vault_to, a.user_to, a.config, result.withdraw),
            ],
            a.token_program,
            &signers,
        )?;

        Ok(())
    }
}
