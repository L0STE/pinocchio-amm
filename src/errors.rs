use pinocchio::error::ProgramError;

#[repr(u32)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum AmmError {
    // Account validation (0-9)
    NotMutable = 0,
    NotSigner = 1,
    InvalidAccountOwner = 2,
    InvalidAccountLength = 3,
    InvalidPda = 4,
    InvalidAuthority = 5,
    InvalidMint = 6,
    InvalidVaultAddress = 7,
    // State errors (10-19)
    PoolNotInitialized = 10,
    PoolLocked = 11,
    // Arithmetic (20-29)
    Overflow = 20,
    Underflow = 21,
    // Business logic (30-39)
    OfferExpired = 30,
    SlippageExceeded = 31,
    InvalidFee = 32,
    InvalidAmount = 33,
    ZeroBalance = 34,
    InsufficientInitialLiquidity = 35,
}

impl From<AmmError> for ProgramError {
    fn from(e: AmmError) -> Self {
        ProgramError::Custom(e as u32)
    }
}
