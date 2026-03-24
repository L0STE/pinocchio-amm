use pinocchio::{
    AccountView, Address,
    error::ProgramError,
    ProgramResult,
};

use crate::constants::CONFIG_SEED;
use crate::errors::AmmError;
use crate::state::Config;

/// Accounts:
///  0. authority  [signer]
///  1. config     [mut]
pub struct ConfigActionAccounts<'a> {
    pub authority: &'a AccountView,
    pub config_account: &'a AccountView,
}

impl<'a> TryFrom<&'a [AccountView]> for ConfigActionAccounts<'a> {
    type Error = ProgramError;

    fn try_from(accounts: &'a [AccountView]) -> Result<Self, ProgramError> {
        let [authority, config_account, ..] = accounts else {
            return Err(ProgramError::NotEnoughAccountKeys);
        };

        if !authority.is_signer() {
            return Err(AmmError::NotSigner.into());
        }

        if !config_account.is_writable() {
            return Err(AmmError::NotMutable.into());
        }

        Ok(Self { authority, config_account })
    }
}

fn validate_config_authority(
    config: &Config,
    config_account: &AccountView,
    authority: &AccountView,
) -> Result<(), ProgramError> {
    let seed_bytes = config.seed().to_le_bytes();
    let (expected, _) = Address::find_program_address(
        &[CONFIG_SEED, &seed_bytes, config.mint_x(), config.mint_y()],
        &crate::ID,
    );
    if expected.ne(config_account.address()) {
        return Err(AmmError::InvalidPda.into());
    }
    if authority.address().as_array() != config.authority() {
        return Err(AmmError::InvalidAuthority.into());
    }
    Ok(())
}

// --- UpdateAuthority ---

pub struct UpdateAuthorityInstructionData {
    pub new_authority: [u8; 32],
}

impl TryFrom<&[u8]> for UpdateAuthorityInstructionData {
    type Error = ProgramError;

    fn try_from(data: &[u8]) -> Result<Self, ProgramError> {
        if data.len() < 32 {
            return Err(ProgramError::InvalidInstructionData);
        }
        let new_authority: [u8; 32] = data[0..32]
            .try_into()
            .map_err(|_| ProgramError::InvalidInstructionData)?;
        Ok(Self { new_authority })
    }
}

pub struct UpdateAuthority<'a> {
    pub accounts: ConfigActionAccounts<'a>,
    pub instruction_data: UpdateAuthorityInstructionData,
}

impl<'a> TryFrom<(&[u8], &'a [AccountView])> for UpdateAuthority<'a> {
    type Error = ProgramError;

    fn try_from((data, accounts): (&[u8], &'a [AccountView])) -> Result<Self, ProgramError> {
        crate::sol_log("UpdateAuthority");

        let accounts = ConfigActionAccounts::try_from(accounts)?;
        let instruction_data = UpdateAuthorityInstructionData::try_from(data)?;

        Ok(Self { accounts, instruction_data })
    }
}

impl<'a> UpdateAuthority<'a> {
    pub const DISCRIMINATOR: &'a u8 = &4;

    pub fn process(&mut self) -> ProgramResult {
        let mut config = Config::load_mut(self.accounts.config_account)?;
        validate_config_authority(&config, self.accounts.config_account, self.accounts.authority)?;
        config.set_authority(&self.instruction_data.new_authority);
        Ok(())
    }
}

// --- UpdateFee ---

pub struct UpdateFeeInstructionData {
    pub fee: u16,
}

impl TryFrom<&[u8]> for UpdateFeeInstructionData {
    type Error = ProgramError;

    fn try_from(data: &[u8]) -> Result<Self, ProgramError> {
        if data.len() < 2 {
            return Err(ProgramError::InvalidInstructionData);
        }
        let fee = u16::from_le_bytes(
            data[0..2]
                .try_into()
                .map_err(|_| ProgramError::InvalidInstructionData)?,
        );
        Ok(Self { fee })
    }
}

pub struct UpdateFee<'a> {
    pub accounts: ConfigActionAccounts<'a>,
    pub instruction_data: UpdateFeeInstructionData,
}

impl<'a> TryFrom<(&[u8], &'a [AccountView])> for UpdateFee<'a> {
    type Error = ProgramError;

    fn try_from((data, accounts): (&[u8], &'a [AccountView])) -> Result<Self, ProgramError> {
        crate::sol_log("UpdateFee");

        let accounts = ConfigActionAccounts::try_from(accounts)?;
        let instruction_data = UpdateFeeInstructionData::try_from(data)?;

        Ok(Self { accounts, instruction_data })
    }
}

impl<'a> UpdateFee<'a> {
    pub const DISCRIMINATOR: &'a u8 = &5;

    pub fn process(&mut self) -> ProgramResult {
        let mut config = Config::load_mut(self.accounts.config_account)?;
        validate_config_authority(&config, self.accounts.config_account, self.accounts.authority)?;
        config.set_fee(self.instruction_data.fee)?;
        Ok(())
    }
}

// --- UpdateLock ---

pub struct UpdateLock<'a> {
    pub accounts: ConfigActionAccounts<'a>,
}

impl<'a> TryFrom<&'a [AccountView]> for UpdateLock<'a> {
    type Error = ProgramError;

    fn try_from(accounts: &'a [AccountView]) -> Result<Self, ProgramError> {
        crate::sol_log("UpdateLock");

        let accounts = ConfigActionAccounts::try_from(accounts)?;

        Ok(Self { accounts })
    }
}

impl<'a> UpdateLock<'a> {
    pub const DISCRIMINATOR: &'a u8 = &6;

    pub fn process(&mut self) -> ProgramResult {
        let mut config = Config::load_mut(self.accounts.config_account)?;
        validate_config_authority(&config, self.accounts.config_account, self.accounts.authority)?;
        let current = config.locked();
        config.set_locked(!current);
        Ok(())
    }
}

// --- RemoveAuthority ---

pub struct RemoveAuthority<'a> {
    pub accounts: ConfigActionAccounts<'a>,
}

impl<'a> TryFrom<&'a [AccountView]> for RemoveAuthority<'a> {
    type Error = ProgramError;

    fn try_from(accounts: &'a [AccountView]) -> Result<Self, ProgramError> {
        crate::sol_log("RemoveAuthority");

        let accounts = ConfigActionAccounts::try_from(accounts)?;

        Ok(Self { accounts })
    }
}

impl<'a> RemoveAuthority<'a> {
    pub const DISCRIMINATOR: &'a u8 = &7;

    pub fn process(&mut self) -> ProgramResult {
        let mut config = Config::load_mut(self.accounts.config_account)?;
        validate_config_authority(&config, self.accounts.config_account, self.accounts.authority)?;
        config.set_authority(&[0u8; 32]);
        Ok(())
    }
}
