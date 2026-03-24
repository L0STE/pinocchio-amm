use pinocchio::account::{Ref, RefMut};
use pinocchio::error::ProgramError;
use pinocchio::AccountView;

use crate::constants::MAX_FEE_BPS;
use crate::errors::AmmError;

#[repr(C)]
pub struct Config {
    discriminator: [u8; 1],
    seed: [u8; 8],
    authority: [u8; 32],
    mint_x: [u8; 32],
    mint_y: [u8; 32],
    fee: [u8; 2],
    locked: [u8; 1],
    lp_bump: [u8; 1],
    bump: [u8; 1],
}

const _: () = assert!(core::mem::size_of::<Config>() == Config::LEN);

impl Config {
    pub const DISCRIMINATOR: u8 = 1;
    pub const LEN: usize = 1 + 8 + 32 + 32 + 32 + 2 + 1 + 1 + 1;

    #[inline(always)]
    pub fn load(account: &AccountView) -> Result<Ref<'_, Self>, ProgramError> {
        if !account.owned_by(&crate::ID) {
            return Err(AmmError::InvalidAccountOwner.into());
        }
        if account.data_len() != Self::LEN {
            return Err(AmmError::InvalidAccountLength.into());
        }
        let data = account.try_borrow()?;
        if data[0] != Self::DISCRIMINATOR {
            return Err(AmmError::PoolNotInitialized.into());
        }
        Ok(Ref::map(data, |d| unsafe { &*(d.as_ptr() as *const Self) }))
    }

    #[inline(always)]
    pub fn load_mut(account: &AccountView) -> Result<RefMut<'_, Self>, ProgramError> {
        if !account.owned_by(&crate::ID) {
            return Err(AmmError::InvalidAccountOwner.into());
        }
        if account.data_len() != Self::LEN {
            return Err(AmmError::InvalidAccountLength.into());
        }
        let data = account.try_borrow_mut()?;
        if data[0] != Self::DISCRIMINATOR {
            return Err(AmmError::PoolNotInitialized.into());
        }
        Ok(RefMut::map(data, |d| unsafe { &mut *(d.as_mut_ptr() as *mut Self) }))
    }

    #[inline(always)]
    pub fn seed(&self) -> u64 {
        u64::from_le_bytes(self.seed)
    }

    #[inline(always)]
    pub fn authority(&self) -> &[u8; 32] {
        &self.authority
    }

    #[inline(always)]
    pub fn mint_x(&self) -> &[u8; 32] {
        &self.mint_x
    }

    #[inline(always)]
    pub fn mint_y(&self) -> &[u8; 32] {
        &self.mint_y
    }

    #[inline(always)]
    pub fn fee(&self) -> u16 {
        u16::from_le_bytes(self.fee)
    }

    #[inline(always)]
    pub fn locked(&self) -> bool {
        self.locked[0] != 0
    }

    #[inline(always)]
    pub fn lp_bump(&self) -> u8 {
        self.lp_bump[0]
    }

    #[inline(always)]
    pub fn bump(&self) -> u8 {
        self.bump[0]
    }

    #[inline(always)]
    pub fn set_seed(&mut self, v: u64) {
        self.seed = v.to_le_bytes();
    }

    #[inline(always)]
    pub fn set_authority(&mut self, v: &[u8; 32]) {
        self.authority = *v;
    }

    #[inline(always)]
    pub fn set_mint_x(&mut self, v: &[u8; 32]) {
        self.mint_x = *v;
    }

    #[inline(always)]
    pub fn set_mint_y(&mut self, v: &[u8; 32]) {
        self.mint_y = *v;
    }

    #[inline(always)]
    pub fn set_fee(&mut self, v: u16) -> Result<(), ProgramError> {
        if v > MAX_FEE_BPS {
            return Err(AmmError::InvalidFee.into());
        }
        self.fee = v.to_le_bytes();
        Ok(())
    }

    #[inline(always)]
    pub fn set_locked(&mut self, v: bool) {
        self.locked[0] = v as u8;
    }

    #[inline(always)]
    pub fn set_lp_bump(&mut self, v: u8) {
        self.lp_bump[0] = v;
    }

    #[inline(always)]
    pub fn set_bump(&mut self, v: u8) {
        self.bump[0] = v;
    }

    /// Initialize all config fields after account creation.
    pub fn init(
        &mut self,
        seed: u64,
        authority: &[u8; 32],
        mint_x: &[u8; 32],
        mint_y: &[u8; 32],
        fee: u16,
        lp_bump: u8,
        bump: u8,
    ) -> Result<(), ProgramError> {
        self.discriminator[0] = Self::DISCRIMINATOR;
        self.set_seed(seed);
        self.set_authority(authority);
        self.set_mint_x(mint_x);
        self.set_mint_y(mint_y);
        self.set_fee(fee)?;
        self.set_locked(false);
        self.set_lp_bump(lp_bump);
        self.set_bump(bump);
        Ok(())
    }
}
