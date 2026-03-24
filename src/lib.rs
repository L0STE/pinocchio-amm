#![no_std]

use pinocchio::{
    AccountView, Address,
    program_entrypoint, no_allocator, nostd_panic_handler,
    error::ProgramError,
    ProgramResult,
};

pub mod batch;
pub mod constants;
pub mod curve;
pub mod errors;
pub mod instructions;
pub mod state;

program_entrypoint!(process_instruction);
no_allocator!();

#[cfg(not(test))]
nostd_panic_handler!();

pub const ID: Address = Address::new_from_array([
    0xee, 0x4e, 0x1b, 0x65, 0xec, 0x86, 0xc0, 0x1e,
    0xb2, 0x2d, 0xb8, 0xc9, 0xa7, 0x5a, 0xc6, 0x69,
    0x79, 0x62, 0x6b, 0x50, 0x59, 0xe3, 0xa8, 0x89,
    0x02, 0x86, 0x4f, 0xd7, 0x7b, 0x0a, 0x00, 0x9e,
]);

#[inline(always)]
pub fn sol_log(msg: &str) {
    #[cfg(target_os = "solana")]
    unsafe {
        pinocchio::syscalls::sol_log_(msg.as_ptr(), msg.len() as u64);
    }
    let _ = msg;
}

use instructions::{
    swap::Swap,
    deposit::Deposit,
    withdraw::Withdraw,
    initialize::Initialize,
    config_actions::{UpdateAuthority, UpdateFee, UpdateLock, RemoveAuthority},
};

fn process_instruction(
    _program_id: &Address,
    accounts: &[AccountView],
    instruction_data: &[u8],
) -> ProgramResult {
    match instruction_data.split_first() {
        Some((Initialize::DISCRIMINATOR, data)) => {
            Initialize::try_from((data, accounts))?.process()
        }
        Some((Deposit::DISCRIMINATOR, data)) => Deposit::try_from((data, accounts))?.process(),
        Some((Withdraw::DISCRIMINATOR, data)) => Withdraw::try_from((data, accounts))?.process(),
        Some((Swap::DISCRIMINATOR, data)) => Swap::try_from((data, accounts))?.process(),
        Some((UpdateAuthority::DISCRIMINATOR, data)) => {
            UpdateAuthority::try_from((data, accounts))?.process()
        }
        Some((UpdateFee::DISCRIMINATOR, data)) => {
            UpdateFee::try_from((data, accounts))?.process()
        }
        Some((UpdateLock::DISCRIMINATOR, _)) => UpdateLock::try_from(accounts)?.process(),
        Some((RemoveAuthority::DISCRIMINATOR, _)) => {
            RemoveAuthority::try_from(accounts)?.process()
        }
        _ => Err(ProgramError::InvalidInstructionData),
    }
}
