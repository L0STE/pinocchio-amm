// Const-generic batch CPI for the p-token program.
//
// Packs N token operations into a single CPI using discriminator 0xFF.
// Const generics let the compiler monomorphize and fully unroll per call site.
//
// Wire format per sub-instruction:
// [0xFF] [num_accts: u8] [data_len: u8] [disc: u8] [amount: u64 LE] ...

use core::mem::MaybeUninit;

use pinocchio::{
    AccountView,
    cpi::{invoke_signed, Signer},
    instruction::{InstructionAccount, InstructionView},
    ProgramResult,
};

const TRANSFER: u8 = 3;
const MINT_TO: u8 = 7;
const BURN: u8 = 8;

// Every sub-ix: 3 accounts, 9 data bytes (1 disc + 8 amount).
// Entry = 2 header bytes + 9 data = 11 bytes.
const ACCTS_PER_OP: u8 = 3;
const DATA_PER_OP: u8 = 9;
const ENTRY: usize = 11;
const MAX_DATA: usize = 1 + 4 * ENTRY;

#[derive(Clone, Copy)]
pub struct TokenOp<'a> {
    a: &'a AccountView,
    b: &'a AccountView,
    authority: &'a AccountView,
    disc: u8,
    amount: u64,
}

impl<'a> TokenOp<'a> {
    #[inline(always)]
    pub fn transfer(
        from: &'a AccountView,
        to: &'a AccountView,
        authority: &'a AccountView,
        amount: u64,
    ) -> Self {
        Self { a: from, b: to, authority, disc: TRANSFER, amount }
    }

    #[inline(always)]
    pub fn mint_to(
        mint: &'a AccountView,
        to: &'a AccountView,
        mint_authority: &'a AccountView,
        amount: u64,
    ) -> Self {
        Self { a: mint, b: to, authority: mint_authority, disc: MINT_TO, amount }
    }

    #[inline(always)]
    pub fn burn(
        from: &'a AccountView,
        mint: &'a AccountView,
        authority: &'a AccountView,
        amount: u64,
    ) -> Self {
        Self { a: from, b: mint, authority, disc: BURN, amount }
    }
}

#[inline(always)]
fn write_entry(buf: &mut [u8], off: usize, disc: u8, amount: u64) {
    buf[off] = ACCTS_PER_OP;
    buf[off + 1] = DATA_PER_OP;
    buf[off + 2] = disc;
    buf[off + 3..off + 11].copy_from_slice(&amount.to_le_bytes());
}

/// Execute N token operations as a single batch CPI.
#[inline(always)]
pub fn batch<'a, const N: usize, const A: usize>(
    ops: [TokenOp<'a>; N],
    token_program: &'a AccountView,
    signers: &[Signer],
) -> ProgramResult {

    let mut data = [0u8; MAX_DATA];
    data[0] = 0xFF;

    let mut accounts = MaybeUninit::<[InstructionAccount; A]>::uninit();
    let mut infos = MaybeUninit::<[&AccountView; A]>::uninit();

    let accts_ptr = accounts.as_mut_ptr() as *mut InstructionAccount;
    let infos_ptr = infos.as_mut_ptr() as *mut &AccountView;

    let mut i = 0;
    while i < N {
        let op = ops[i];
        write_entry(&mut data, 1 + i * ENTRY, op.disc, op.amount);

        unsafe {
            core::ptr::write(accts_ptr.add(i * 3), InstructionAccount::writable(op.a.address()));
            core::ptr::write(accts_ptr.add(i * 3 + 1), InstructionAccount::writable(op.b.address()));
            core::ptr::write(accts_ptr.add(i * 3 + 2), InstructionAccount::readonly_signer(op.authority.address()));
            core::ptr::write(infos_ptr.add(i * 3), op.a);
            core::ptr::write(infos_ptr.add(i * 3 + 1), op.b);
            core::ptr::write(infos_ptr.add(i * 3 + 2), op.authority);
        }

        i += 1;
    }

    invoke_signed(
        &InstructionView {
            program_id: token_program.address(),
            accounts: unsafe { &*accounts.as_ptr() },
            data: &data[..1 + N * ENTRY],
        },
        unsafe { &*infos.as_ptr() },
        signers,
    )
}
