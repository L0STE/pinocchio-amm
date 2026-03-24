use litesvm::LiteSVM;
use solana_account::Account;
use solana_clock::Clock;
use solana_instruction::{AccountMeta, Instruction};
use solana_keypair::Keypair;
use solana_message::Message;
use solana_pubkey::Pubkey;
use solana_rent::Rent;
use solana_signer::Signer;
use solana_transaction::Transaction;
use spl_associated_token_account::get_associated_token_address;

// =============================================================================
// Program IDs
// =============================================================================

pub const PROGRAM_ID: Pubkey = solana_pubkey::pubkey!("H3F7Q6otZ1wA9jt35TM7LzEbVThpFVFMLr3RBbUzZohf");
pub const TOKEN_PROGRAM_ID: Pubkey =
    solana_pubkey::pubkey!("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA");
pub const SYSTEM_PROGRAM_ID: Pubkey =
    solana_pubkey::pubkey!("11111111111111111111111111111111");
pub const ATA_PROGRAM_ID: Pubkey =
    solana_pubkey::pubkey!("ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL");

// =============================================================================
// Discriminators
// =============================================================================

pub const DISC_INITIALIZE: u8 = 0;
pub const DISC_DEPOSIT: u8 = 1;
pub const DISC_WITHDRAW: u8 = 2;
pub const DISC_SWAP: u8 = 3;
pub const DISC_UPDATE_AUTHORITY: u8 = 4;
pub const DISC_UPDATE_FEE: u8 = 5;
pub const DISC_UPDATE_LOCK: u8 = 6;
pub const DISC_REMOVE_AUTHORITY: u8 = 7;

// =============================================================================
// Config State Layout (110 bytes)
// =============================================================================

pub const CONFIG_LEN: usize = 110;
pub const CONFIG_DISCRIMINATOR: u8 = 1;

// Offsets into Config account data
pub const CONFIG_DISC_OFF: usize = 0;
pub const CONFIG_SEED_OFF: usize = 1;
pub const CONFIG_AUTH_OFF: usize = 9;
pub const CONFIG_MINT_X_OFF: usize = 41;
pub const CONFIG_MINT_Y_OFF: usize = 73;
pub const CONFIG_FEE_OFF: usize = 105;
pub const CONFIG_LOCKED_OFF: usize = 107;
pub const CONFIG_LP_BUMP_OFF: usize = 108;
pub const CONFIG_BUMP_OFF: usize = 109;

// =============================================================================
// SVM Setup
// =============================================================================

/// Create LiteSVM with our AMM program + p-token (replaces default SPL Token)
pub fn setup_svm() -> LiteSVM {
    let mut svm = LiteSVM::new();
    svm.add_program(
        PROGRAM_ID,
        include_bytes!("../../target/deploy/pinocchio_amm.so"),
    ).unwrap();
    // Load p-token (pinocchio token program) at the SPL Token address
    svm.add_program(
        TOKEN_PROGRAM_ID,
        include_bytes!("../fixtures/programs/pinocchio_token_program.so"),
    ).unwrap();
    svm
}

/// Create LiteSVM with clock set for expiration testing
pub fn setup_svm_with_clock(unix_timestamp: i64) -> LiteSVM {
    let mut svm = setup_svm();
    let slot = 100;
    svm.warp_to_slot(slot);
    let clock = Clock {
        slot,
        epoch_start_timestamp: unix_timestamp,
        epoch: 100,
        leader_schedule_epoch: 101,
        unix_timestamp,
    };
    svm.set_sysvar(&clock);
    svm
}

// =============================================================================
// Mint / Token Account Helpers (mocked for setup, real CPI for AMM ops)
// =============================================================================

/// Create a mock mint account (external mints like USDC/SOL are pre-existing)
pub fn create_mint(svm: &mut LiteSVM, mint: &Pubkey, authority: &Pubkey, decimals: u8) {
    let mut data = vec![0u8; 82];
    // COption::Some for mint_authority
    data[0..4].copy_from_slice(&1u32.to_le_bytes());
    data[4..36].copy_from_slice(authority.as_ref());
    // supply = 0
    data[36..44].copy_from_slice(&0u64.to_le_bytes());
    data[44] = decimals;
    data[45] = 1; // is_initialized = true
    // No freeze authority (COption::None = 0)

    svm.set_account(
        *mint,
        Account {
            lamports: Rent::default().minimum_balance(82),
            data,
            owner: TOKEN_PROGRAM_ID,
            executable: false,
            rent_epoch: 0,
        }
        .into(),
    )
    .unwrap();
}

/// Create a mock token account (ATA) with initial balance
pub fn create_token_account(
    svm: &mut LiteSVM,
    owner: &Pubkey,
    mint: &Pubkey,
    amount: u64,
) -> Pubkey {
    let ata = get_associated_token_address(owner, mint);
    create_token_account_at(svm, &ata, owner, mint, amount);
    ata
}

/// Create a token account at a specific address
pub fn create_token_account_at(
    svm: &mut LiteSVM,
    pubkey: &Pubkey,
    owner: &Pubkey,
    mint: &Pubkey,
    amount: u64,
) {
    let mut data = vec![0u8; 165];
    data[0..32].copy_from_slice(mint.as_ref());
    data[32..64].copy_from_slice(owner.as_ref());
    data[64..72].copy_from_slice(&amount.to_le_bytes());
    // state = Initialized
    data[108] = 1;

    svm.set_account(
        *pubkey,
        Account {
            lamports: Rent::default().minimum_balance(165),
            data,
            owner: TOKEN_PROGRAM_ID,
            executable: false,
            rent_epoch: 0,
        }
        .into(),
    )
    .unwrap();
}

/// Update a mock mint's supply (needed after mocking token accounts)
pub fn set_mint_supply(svm: &mut LiteSVM, mint: &Pubkey, supply: u64) {
    let mut account = svm.get_account(mint).unwrap();
    account.data[36..44].copy_from_slice(&supply.to_le_bytes());
    svm.set_account(*mint, account.into()).unwrap();
}

// =============================================================================
// Account Data Readers
// =============================================================================

pub fn get_token_balance(svm: &LiteSVM, token_account: &Pubkey) -> u64 {
    let account = svm.get_account(token_account).expect("Token account not found");
    u64::from_le_bytes(account.data[64..72].try_into().unwrap())
}

pub fn get_mint_supply(svm: &LiteSVM, mint: &Pubkey) -> u64 {
    let account = svm.get_account(mint).expect("Mint not found");
    u64::from_le_bytes(account.data[36..44].try_into().unwrap())
}

pub fn get_config_data(svm: &LiteSVM, config: &Pubkey) -> Vec<u8> {
    svm.get_account(config).expect("Config not found").data
}

pub fn get_config_fee(data: &[u8]) -> u16 {
    u16::from_le_bytes(data[CONFIG_FEE_OFF..CONFIG_FEE_OFF + 2].try_into().unwrap())
}

pub fn get_config_locked(data: &[u8]) -> bool {
    data[CONFIG_LOCKED_OFF] != 0
}

pub fn get_config_authority(data: &[u8]) -> [u8; 32] {
    data[CONFIG_AUTH_OFF..CONFIG_AUTH_OFF + 32].try_into().unwrap()
}

// =============================================================================
// PDA Derivation
// =============================================================================

pub fn derive_config_pda(seed: u64, mint_x: &Pubkey, mint_y: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[b"config", &seed.to_le_bytes(), mint_x.as_ref(), mint_y.as_ref()],
        &PROGRAM_ID,
    )
}

pub fn derive_lp_mint_pda(config: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[b"lp", config.as_ref()], &PROGRAM_ID)
}

// =============================================================================
// Instruction Builders
// =============================================================================

pub fn build_initialize_ix(
    initializer: &Pubkey,
    mint_x: &Pubkey,
    mint_y: &Pubkey,
    seed: u64,
    authority: &Pubkey,
    fee: u16,
    init_amount_x: u64,
    init_amount_y: u64,
) -> Instruction {
    let (config_pda, _) = derive_config_pda(seed, mint_x, mint_y);
    let (lp_mint, _) = derive_lp_mint_pda(&config_pda);
    let vault_x = get_associated_token_address(&config_pda, mint_x);
    let vault_y = get_associated_token_address(&config_pda, mint_y);
    let initializer_x = get_associated_token_address(initializer, mint_x);
    let initializer_y = get_associated_token_address(initializer, mint_y);
    let initializer_lp = get_associated_token_address(initializer, &lp_mint);

    let mut data = vec![DISC_INITIALIZE];
    data.extend_from_slice(&seed.to_le_bytes());
    data.extend_from_slice(authority.as_ref());
    data.extend_from_slice(&fee.to_le_bytes());
    data.extend_from_slice(&init_amount_x.to_le_bytes());
    data.extend_from_slice(&init_amount_y.to_le_bytes());

    Instruction {
        program_id: PROGRAM_ID,
        accounts: vec![
            AccountMeta::new(*initializer, true),
            AccountMeta::new_readonly(*mint_x, false),
            AccountMeta::new_readonly(*mint_y, false),
            AccountMeta::new(lp_mint, false),
            AccountMeta::new(vault_x, false),
            AccountMeta::new(vault_y, false),
            AccountMeta::new(initializer_x, false),
            AccountMeta::new(initializer_y, false),
            AccountMeta::new(initializer_lp, false),
            AccountMeta::new(config_pda, false),
            AccountMeta::new_readonly(SYSTEM_PROGRAM_ID, false),
            AccountMeta::new_readonly(TOKEN_PROGRAM_ID, false),
            AccountMeta::new_readonly(ATA_PROGRAM_ID, false),
        ],
        data,
    }
}

pub fn build_deposit_ix(
    user: &Pubkey,
    mint_x: &Pubkey,
    mint_y: &Pubkey,
    config: &Pubkey,
    amount: u64,
    max_x: u64,
    max_y: u64,
    expiration: i64,
) -> Instruction {
    let (lp_mint, _) = derive_lp_mint_pda(config);
    let vault_x = get_associated_token_address(config, mint_x);
    let vault_y = get_associated_token_address(config, mint_y);
    let user_x = get_associated_token_address(user, mint_x);
    let user_y = get_associated_token_address(user, mint_y);
    let user_lp = get_associated_token_address(user, &lp_mint);

    let mut data = vec![DISC_DEPOSIT];
    data.extend_from_slice(&amount.to_le_bytes());
    data.extend_from_slice(&max_x.to_le_bytes());
    data.extend_from_slice(&max_y.to_le_bytes());
    data.extend_from_slice(&expiration.to_le_bytes());

    Instruction {
        program_id: PROGRAM_ID,
        accounts: vec![
            AccountMeta::new(*user, true),
            AccountMeta::new_readonly(*mint_x, false),
            AccountMeta::new_readonly(*mint_y, false),
            AccountMeta::new(lp_mint, false),
            AccountMeta::new(vault_x, false),
            AccountMeta::new(vault_y, false),
            AccountMeta::new(user_x, false),
            AccountMeta::new(user_y, false),
            AccountMeta::new(user_lp, false),
            AccountMeta::new_readonly(*config, false),
            AccountMeta::new_readonly(TOKEN_PROGRAM_ID, false),
        ],
        data,
    }
}

pub fn build_withdraw_ix(
    user: &Pubkey,
    mint_x: &Pubkey,
    mint_y: &Pubkey,
    config: &Pubkey,
    amount: u64,
    min_x: u64,
    min_y: u64,
    expiration: i64,
) -> Instruction {
    let (lp_mint, _) = derive_lp_mint_pda(config);
    let vault_x = get_associated_token_address(config, mint_x);
    let vault_y = get_associated_token_address(config, mint_y);
    let user_x = get_associated_token_address(user, mint_x);
    let user_y = get_associated_token_address(user, mint_y);
    let user_lp = get_associated_token_address(user, &lp_mint);

    let mut data = vec![DISC_WITHDRAW];
    data.extend_from_slice(&amount.to_le_bytes());
    data.extend_from_slice(&min_x.to_le_bytes());
    data.extend_from_slice(&min_y.to_le_bytes());
    data.extend_from_slice(&expiration.to_le_bytes());

    Instruction {
        program_id: PROGRAM_ID,
        accounts: vec![
            AccountMeta::new(*user, true),
            AccountMeta::new_readonly(*mint_x, false),
            AccountMeta::new_readonly(*mint_y, false),
            AccountMeta::new(lp_mint, false),
            AccountMeta::new(vault_x, false),
            AccountMeta::new(vault_y, false),
            AccountMeta::new(user_x, false),
            AccountMeta::new(user_y, false),
            AccountMeta::new(user_lp, false),
            AccountMeta::new_readonly(*config, false),
            AccountMeta::new_readonly(TOKEN_PROGRAM_ID, false),
        ],
        data,
    }
}

pub fn build_swap_ix(
    user: &Pubkey,
    mint_from: &Pubkey,
    mint_to: &Pubkey,
    config: &Pubkey,
    amount: u64,
    min: u64,
    expiration: i64,
) -> Instruction {
    let vault_from = get_associated_token_address(config, mint_from);
    let vault_to = get_associated_token_address(config, mint_to);
    let user_from = get_associated_token_address(user, mint_from);
    let user_to = get_associated_token_address(user, mint_to);

    let mut data = vec![DISC_SWAP];
    data.extend_from_slice(&amount.to_le_bytes());
    data.extend_from_slice(&min.to_le_bytes());
    data.extend_from_slice(&expiration.to_le_bytes());

    Instruction {
        program_id: PROGRAM_ID,
        accounts: vec![
            AccountMeta::new(*user, true),
            AccountMeta::new_readonly(*mint_from, false),
            AccountMeta::new_readonly(*mint_to, false),
            AccountMeta::new(user_from, false),
            AccountMeta::new(user_to, false),
            AccountMeta::new(vault_from, false),
            AccountMeta::new(vault_to, false),
            AccountMeta::new_readonly(*config, false),
            AccountMeta::new_readonly(TOKEN_PROGRAM_ID, false),
        ],
        data,
    }
}

pub fn build_update_authority_ix(
    authority: &Pubkey,
    config: &Pubkey,
    new_authority: &Pubkey,
) -> Instruction {
    let mut data = vec![DISC_UPDATE_AUTHORITY];
    data.extend_from_slice(new_authority.as_ref());

    Instruction {
        program_id: PROGRAM_ID,
        accounts: vec![
            AccountMeta::new_readonly(*authority, true),
            AccountMeta::new(*config, false),
        ],
        data,
    }
}

pub fn build_update_fee_ix(authority: &Pubkey, config: &Pubkey, fee: u16) -> Instruction {
    let mut data = vec![DISC_UPDATE_FEE];
    data.extend_from_slice(&fee.to_le_bytes());

    Instruction {
        program_id: PROGRAM_ID,
        accounts: vec![
            AccountMeta::new_readonly(*authority, true),
            AccountMeta::new(*config, false),
        ],
        data,
    }
}

pub fn build_update_lock_ix(authority: &Pubkey, config: &Pubkey) -> Instruction {
    Instruction {
        program_id: PROGRAM_ID,
        accounts: vec![
            AccountMeta::new_readonly(*authority, true),
            AccountMeta::new(*config, false),
        ],
        data: vec![DISC_UPDATE_LOCK],
    }
}

pub fn build_remove_authority_ix(authority: &Pubkey, config: &Pubkey) -> Instruction {
    Instruction {
        program_id: PROGRAM_ID,
        accounts: vec![
            AccountMeta::new_readonly(*authority, true),
            AccountMeta::new(*config, false),
        ],
        data: vec![DISC_REMOVE_AUTHORITY],
    }
}

// =============================================================================
// Test Context — Initialized pool state for deposit/withdraw/swap tests
// =============================================================================

pub struct PoolContext {
    pub svm: LiteSVM,
    pub initializer: Keypair,
    pub mint_x: Pubkey,
    pub mint_y: Pubkey,
    pub config: Pubkey,
    pub lp_mint: Pubkey,
    pub seed: u64,
    pub fee: u16,
}

impl PoolContext {
    /// Create a fully initialized pool with given parameters.
    /// Uses mock mints + user token accounts, then calls Initialize through real CPI.
    pub fn new(
        seed: u64,
        fee: u16,
        init_x: u64,
        init_y: u64,
        clock_timestamp: i64,
    ) -> Self {
        let mut svm = setup_svm_with_clock(clock_timestamp);

        let initializer = Keypair::new();
        svm.airdrop(&initializer.pubkey(), 100_000_000_000).unwrap();

        let mint_x = Keypair::new().pubkey();
        let mint_y = Keypair::new().pubkey();

        // Create external mints (mock — these exist before our program)
        create_mint(&mut svm, &mint_x, &initializer.pubkey(), 6);
        create_mint(&mut svm, &mint_y, &initializer.pubkey(), 6);

        // Create user token accounts with initial tokens (mock)
        create_token_account(&mut svm, &initializer.pubkey(), &mint_x, init_x);
        create_token_account(&mut svm, &initializer.pubkey(), &mint_y, init_y);

        // Set mint supply to match mocked balances
        set_mint_supply(&mut svm, &mint_x, init_x);
        set_mint_supply(&mut svm, &mint_y, init_y);

        // Build and execute Initialize (real CPI through p-token)
        let ix = build_initialize_ix(
            &initializer.pubkey(),
            &mint_x,
            &mint_y,
            seed,
            &initializer.pubkey(),
            fee,
            init_x,
            init_y,
        );

        let tx = Transaction::new(
            &[&initializer],
            Message::new(&[ix], Some(&initializer.pubkey())),
            svm.latest_blockhash(),
        );
        svm.send_transaction(tx).unwrap();

        let (config, _) = derive_config_pda(seed, &mint_x, &mint_y);
        let (lp_mint, _) = derive_lp_mint_pda(&config);

        Self {
            svm,
            initializer,
            mint_x,
            mint_y,
            config,
            lp_mint,
            seed,
            fee,
        }
    }

    /// Add a new user with token balances for x, y, and optionally LP
    pub fn add_user(&mut self, amount_x: u64, amount_y: u64) -> Keypair {
        let user = Keypair::new();
        self.svm.airdrop(&user.pubkey(), 10_000_000_000).unwrap();

        if amount_x > 0 {
            create_token_account(&mut self.svm, &user.pubkey(), &self.mint_x, amount_x);
        }
        if amount_y > 0 {
            create_token_account(&mut self.svm, &user.pubkey(), &self.mint_y, amount_y);
        }

        user
    }

    /// Create user LP token account with given balance
    pub fn add_user_lp(&mut self, user: &Pubkey, amount: u64) -> Pubkey {
        create_token_account(&mut self.svm, user, &self.lp_mint, amount)
    }
}

// =============================================================================
// Transaction Helpers
// =============================================================================

pub fn send_tx(svm: &mut LiteSVM, payer: &Keypair, ix: Instruction) -> Result<(), String> {
    let tx = Transaction::new(
        &[payer],
        Message::new(&[ix], Some(&payer.pubkey())),
        svm.latest_blockhash(),
    );
    svm.send_transaction(tx).map(|_| ()).map_err(|e| format!("{:?}", e))
}

/// Extract custom error code from a failed transaction error string
pub fn get_custom_error(err: &str) -> Option<u32> {
    if let Some(pos) = err.find("Custom(") {
        let after = &err[pos + 7..];
        if let Some(end) = after.find(')') {
            return after[..end].parse().ok();
        }
    }
    None
}
