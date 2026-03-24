use pinocchio::{
    AccountView, Address,
    cpi::{Seed, Signer},
    error::ProgramError,
    ProgramResult,
};
use pinocchio_system::create_account_with_minimum_balance_signed;
use pinocchio_associated_token_account::instructions::Create as CreateAta;
use pinocchio_token::instructions::InitializeMint2;
use crate::batch::{batch, TokenOp};
use crate::constants::{CONFIG_SEED, LP_SEED, MAX_FEE_BPS, MINIMUM_LIQUIDITY};
use crate::curve;
use crate::errors::AmmError;
use crate::state::Config;

const MINT_ACCOUNT_SIZE: usize = 82;
const DATA_LEN: usize = 58;

/// Accounts:
///  0. initializer     [signer, mut]
///  1. mint_x          []
///  2. mint_y          []
///  3. mint_lp         [mut]
///  4. vault_x         [mut]
///  5. vault_y         [mut]
///  6. initializer_x   [mut]
///  7. initializer_y   [mut]
///  8. initializer_lp  [mut]
///  9. config          [mut]
/// 10. system_program  []
/// 11. token_program   []
/// 12. ata_program     []
pub struct InitializeAccounts<'a> {
    pub initializer: &'a AccountView,
    pub mint_x: &'a AccountView,
    pub mint_y: &'a AccountView,
    pub mint_lp: &'a AccountView,
    pub vault_x: &'a AccountView,
    pub vault_y: &'a AccountView,
    pub initializer_x: &'a AccountView,
    pub initializer_y: &'a AccountView,
    pub initializer_lp: &'a AccountView,
    pub config: &'a AccountView,
    pub system_program: &'a AccountView,
    pub token_program: &'a AccountView,
}

impl<'a> TryFrom<&'a [AccountView]> for InitializeAccounts<'a> {
    type Error = ProgramError;

    fn try_from(accounts: &'a [AccountView]) -> Result<Self, ProgramError> {
        let [
            initializer, mint_x, mint_y, mint_lp,
            vault_x, vault_y,
            initializer_x, initializer_y, initializer_lp,
            config, system_program, token_program, _ata_program,
            ..
        ] = accounts else {
            return Err(ProgramError::NotEnoughAccountKeys);
        };

        if !initializer.is_signer() {
            return Err(AmmError::NotSigner.into());
        }

        Ok(Self {
            initializer, mint_x, mint_y, mint_lp,
            vault_x, vault_y,
            initializer_x, initializer_y, initializer_lp,
            config, system_program, token_program,
        })
    }
}

pub struct InitializeInstructionData {
    pub seed: u64,
    pub authority: [u8; 32],
    pub fee: u16,
    pub init_amount_x: u64,
    pub init_amount_y: u64,
}

impl TryFrom<&[u8]> for InitializeInstructionData {
    type Error = ProgramError;

    fn try_from(data: &[u8]) -> Result<Self, ProgramError> {
        if data.len() < DATA_LEN {
            return Err(ProgramError::InvalidInstructionData);
        }

        let seed = u64::from_le_bytes(
            data[0..8].try_into().map_err(|_| ProgramError::InvalidInstructionData)?,
        );
        let authority: [u8; 32] = data[8..40]
            .try_into()
            .map_err(|_| ProgramError::InvalidInstructionData)?;
        let fee = u16::from_le_bytes(
            data[40..42].try_into().map_err(|_| ProgramError::InvalidInstructionData)?,
        );
        let init_amount_x = u64::from_le_bytes(
            data[42..50].try_into().map_err(|_| ProgramError::InvalidInstructionData)?,
        );
        let init_amount_y = u64::from_le_bytes(
            data[50..58].try_into().map_err(|_| ProgramError::InvalidInstructionData)?,
        );

        if fee > MAX_FEE_BPS {
            return Err(AmmError::InvalidFee.into());
        }
        if init_amount_x == 0 || init_amount_y == 0 {
            return Err(AmmError::InvalidAmount.into());
        }

        Ok(Self { seed, authority, fee, init_amount_x, init_amount_y })
    }
}

pub struct Initialize<'a> {
    pub accounts: InitializeAccounts<'a>,
    pub instruction_data: InitializeInstructionData,
    pub config_bump: u8,
    pub lp_bump: u8,
}

impl<'a> TryFrom<(&[u8], &'a [AccountView])> for Initialize<'a> {
    type Error = ProgramError;

    fn try_from((data, accounts): (&[u8], &'a [AccountView])) -> Result<Self, ProgramError> {
        crate::sol_log("Initialize");

        let accounts = InitializeAccounts::try_from(accounts)?;
        let instruction_data = InitializeInstructionData::try_from(data)?;

        let seed_bytes = instruction_data.seed.to_le_bytes();
        let (config_pda, config_bump) = Address::find_program_address(
            &[CONFIG_SEED, &seed_bytes, accounts.mint_x.address().as_ref(), accounts.mint_y.address().as_ref()],
            &crate::ID,
        );
        if config_pda.ne(accounts.config.address()) {
            return Err(AmmError::InvalidPda.into());
        }

        let (lp_pda, lp_bump) = Address::find_program_address(
            &[LP_SEED, accounts.config.address().as_ref()],
            &crate::ID,
        );
        if lp_pda.ne(accounts.mint_lp.address()) {
            return Err(AmmError::InvalidPda.into());
        }

        Ok(Self {
            accounts,
            instruction_data,
            config_bump,
            lp_bump,
        })
    }
}

impl<'a> Initialize<'a> {
    pub const DISCRIMINATOR: &'a u8 = &0;

    pub fn process(&mut self) -> ProgramResult {
        let a = &self.accounts;

        let seed_bytes = self.instruction_data.seed.to_le_bytes();
        let config_bump_ref = [self.config_bump];
        let config_seeds = [
            Seed::from(CONFIG_SEED),
            Seed::from(seed_bytes.as_ref()),
            Seed::from(a.mint_x.address().as_ref()),
            Seed::from(a.mint_y.address().as_ref()),
            Seed::from(&config_bump_ref),
        ];
        let config_signer = [Signer::from(&config_seeds)];

        create_account_with_minimum_balance_signed(
            a.config,
            Config::LEN,
            &crate::ID,
            a.initializer,
            None,
            &config_signer,
        )?;

        let lp_bump_ref = [self.lp_bump];
        let lp_seeds = [
            Seed::from(LP_SEED),
            Seed::from(a.config.address().as_ref()),
            Seed::from(&lp_bump_ref),
        ];
        let lp_signer = [Signer::from(&lp_seeds)];

        create_account_with_minimum_balance_signed(
            a.mint_lp,
            MINT_ACCOUNT_SIZE,
            a.token_program.address(),
            a.initializer,
            None,
            &lp_signer,
        )?;

        InitializeMint2 {
            mint: a.mint_lp,
            decimals: 6,
            mint_authority: a.config.address(),
            freeze_authority: None,
        }
        .invoke()?;

        CreateAta {
            funding_account: a.initializer,
            account: a.vault_x,
            wallet: a.config,
            mint: a.mint_x,
            system_program: a.system_program,
            token_program: a.token_program,
        }
        .invoke()?;

        CreateAta {
            funding_account: a.initializer,
            account: a.vault_y,
            wallet: a.config,
            mint: a.mint_y,
            system_program: a.system_program,
            token_program: a.token_program,
        }
        .invoke()?;

        CreateAta {
            funding_account: a.initializer,
            account: a.initializer_lp,
            wallet: a.initializer,
            mint: a.mint_lp,
            system_program: a.system_program,
            token_program: a.token_program,
        }
        .invoke()?;

        {
            let mut data = a.config.try_borrow_mut()?;
            let state = unsafe { &mut *(data.as_mut_ptr() as *mut Config) };
            state.init(
                self.instruction_data.seed,
                &self.instruction_data.authority,
                a.mint_x.address().as_array(),
                a.mint_y.address().as_array(),
                self.instruction_data.fee,
                self.lp_bump,
                self.config_bump,
            )?;
        }

        let product = (self.instruction_data.init_amount_x as u128)
            .checked_mul(self.instruction_data.init_amount_y as u128)
            .ok_or(AmmError::Overflow)?;
        let liquidity = curve::integer_sqrt(product) as u64;

        if liquidity <= MINIMUM_LIQUIDITY {
            return Err(AmmError::InsufficientInitialLiquidity.into());
        }

        let lp_to_mint = liquidity
            .checked_sub(MINIMUM_LIQUIDITY)
            .ok_or(AmmError::Underflow)?;

        batch::<3, 9>(
            [
                TokenOp::transfer(a.initializer_x, a.vault_x, a.initializer, self.instruction_data.init_amount_x),
                TokenOp::transfer(a.initializer_y, a.vault_y, a.initializer, self.instruction_data.init_amount_y),
                TokenOp::mint_to(a.mint_lp, a.initializer_lp, a.config, lp_to_mint),
            ],
            a.token_program,
            &config_signer,
        )?;

        Ok(())
    }
}
