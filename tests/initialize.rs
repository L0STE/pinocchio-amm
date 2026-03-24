use crate::common::*;
use solana_keypair::Keypair;
use solana_message::Message;
use solana_signer::Signer;
use solana_transaction::Transaction;
use spl_associated_token_account::get_associated_token_address;

// =============================================================================
// HAPPY PATH
// =============================================================================

#[test]
fn test_initialize_success() {
    let seed: u64 = 42;
    let fee: u16 = 300; // 3%
    let init_x: u64 = 1_000_000; // 1 token (6 decimals)
    let init_y: u64 = 2_000_000;

    let mut svm = setup_svm_with_clock(1_000_000);
    let initializer = Keypair::new();
    svm.airdrop(&initializer.pubkey(), 100_000_000_000).unwrap();

    let mint_x = Keypair::new().pubkey();
    let mint_y = Keypair::new().pubkey();

    create_mint(&mut svm, &mint_x, &initializer.pubkey(), 6);
    create_mint(&mut svm, &mint_y, &initializer.pubkey(), 6);
    create_token_account(&mut svm, &initializer.pubkey(), &mint_x, init_x);
    create_token_account(&mut svm, &initializer.pubkey(), &mint_y, init_y);
    set_mint_supply(&mut svm, &mint_x, init_x);
    set_mint_supply(&mut svm, &mint_y, init_y);

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
    let result = svm.send_transaction(tx);
    assert!(result.is_ok(), "Initialize failed: {:?}", result.err());

    // --- Verify config state ---
    let (config_pda, _) = derive_config_pda(seed, &mint_x, &mint_y);
    let data = get_config_data(&svm, &config_pda);

    assert_eq!(data.len(), CONFIG_LEN);
    assert_eq!(data[CONFIG_DISC_OFF], CONFIG_DISCRIMINATOR);
    assert_eq!(
        u64::from_le_bytes(data[CONFIG_SEED_OFF..CONFIG_SEED_OFF + 8].try_into().unwrap()),
        seed
    );
    assert_eq!(
        &data[CONFIG_AUTH_OFF..CONFIG_AUTH_OFF + 32],
        initializer.pubkey().as_ref()
    );
    assert_eq!(
        &data[CONFIG_MINT_X_OFF..CONFIG_MINT_X_OFF + 32],
        mint_x.as_ref()
    );
    assert_eq!(
        &data[CONFIG_MINT_Y_OFF..CONFIG_MINT_Y_OFF + 32],
        mint_y.as_ref()
    );
    assert_eq!(get_config_fee(&data), fee);
    assert!(!get_config_locked(&data));

    // --- Verify vault balances ---
    let vault_x = get_associated_token_address(&config_pda, &mint_x);
    let vault_y = get_associated_token_address(&config_pda, &mint_y);
    assert_eq!(get_token_balance(&svm, &vault_x), init_x);
    assert_eq!(get_token_balance(&svm, &vault_y), init_y);

    // --- Verify LP tokens minted ---
    // LP = sqrt(init_x * init_y) - MINIMUM_LIQUIDITY(1000)
    let expected_lp = integer_sqrt(init_x as u128 * init_y as u128) as u64 - 1000;
    let (lp_mint, _) = derive_lp_mint_pda(&config_pda);
    let initializer_lp = get_associated_token_address(&initializer.pubkey(), &lp_mint);
    assert_eq!(get_token_balance(&svm, &initializer_lp), expected_lp);

    // --- Verify user token balances drained ---
    let user_x = get_associated_token_address(&initializer.pubkey(), &mint_x);
    let user_y = get_associated_token_address(&initializer.pubkey(), &mint_y);
    assert_eq!(get_token_balance(&svm, &user_x), 0);
    assert_eq!(get_token_balance(&svm, &user_y), 0);
}

// =============================================================================
// ERROR PATHS
// =============================================================================

#[test]
fn test_initialize_fails_with_zero_amount_x() {
    let mut svm = setup_svm_with_clock(1_000_000);
    let user = Keypair::new();
    svm.airdrop(&user.pubkey(), 10_000_000_000).unwrap();

    let mint_x = Keypair::new().pubkey();
    let mint_y = Keypair::new().pubkey();
    create_mint(&mut svm, &mint_x, &user.pubkey(), 6);
    create_mint(&mut svm, &mint_y, &user.pubkey(), 6);
    create_token_account(&mut svm, &user.pubkey(), &mint_x, 1_000_000);
    create_token_account(&mut svm, &user.pubkey(), &mint_y, 1_000_000);

    let ix = build_initialize_ix(&user.pubkey(), &mint_x, &mint_y, 1, &user.pubkey(), 300, 0, 1_000_000);

    let tx = Transaction::new(
        &[&user],
        Message::new(&[ix], Some(&user.pubkey())),
        svm.latest_blockhash(),
    );
    let result = svm.send_transaction(tx);
    assert!(result.is_err(), "Should fail with zero init_amount_x");
}

#[test]
fn test_initialize_fails_with_excessive_fee() {
    let mut svm = setup_svm_with_clock(1_000_000);
    let user = Keypair::new();
    svm.airdrop(&user.pubkey(), 10_000_000_000).unwrap();

    let mint_x = Keypair::new().pubkey();
    let mint_y = Keypair::new().pubkey();
    create_mint(&mut svm, &mint_x, &user.pubkey(), 6);
    create_mint(&mut svm, &mint_y, &user.pubkey(), 6);
    create_token_account(&mut svm, &user.pubkey(), &mint_x, 1_000_000);
    create_token_account(&mut svm, &user.pubkey(), &mint_y, 1_000_000);

    // fee = 10001 (> 100%)
    let ix = build_initialize_ix(
        &user.pubkey(), &mint_x, &mint_y, 1, &user.pubkey(), 10_001, 1_000_000, 1_000_000,
    );

    let tx = Transaction::new(
        &[&user],
        Message::new(&[ix], Some(&user.pubkey())),
        svm.latest_blockhash(),
    );
    assert!(svm.send_transaction(tx).is_err(), "Should fail with excessive fee");
}

#[test]
fn test_initialize_fails_with_insufficient_liquidity() {
    let mut svm = setup_svm_with_clock(1_000_000);
    let user = Keypair::new();
    svm.airdrop(&user.pubkey(), 10_000_000_000).unwrap();

    let mint_x = Keypair::new().pubkey();
    let mint_y = Keypair::new().pubkey();
    create_mint(&mut svm, &mint_x, &user.pubkey(), 6);
    create_mint(&mut svm, &mint_y, &user.pubkey(), 6);
    // sqrt(1 * 1) = 1 which is <= MINIMUM_LIQUIDITY(1000)
    create_token_account(&mut svm, &user.pubkey(), &mint_x, 1);
    create_token_account(&mut svm, &user.pubkey(), &mint_y, 1);
    set_mint_supply(&mut svm, &mint_x, 1);
    set_mint_supply(&mut svm, &mint_y, 1);

    let ix = build_initialize_ix(&user.pubkey(), &mint_x, &mint_y, 1, &user.pubkey(), 300, 1, 1);

    let tx = Transaction::new(
        &[&user],
        Message::new(&[ix], Some(&user.pubkey())),
        svm.latest_blockhash(),
    );
    assert!(svm.send_transaction(tx).is_err(), "Should fail with insufficient initial liquidity");
}

#[test]
fn test_initialize_with_max_fee_succeeds() {
    let mut svm = setup_svm_with_clock(1_000_000);
    let user = Keypair::new();
    svm.airdrop(&user.pubkey(), 100_000_000_000).unwrap();

    let mint_x = Keypair::new().pubkey();
    let mint_y = Keypair::new().pubkey();
    create_mint(&mut svm, &mint_x, &user.pubkey(), 6);
    create_mint(&mut svm, &mint_y, &user.pubkey(), 6);
    create_token_account(&mut svm, &user.pubkey(), &mint_x, 1_000_000);
    create_token_account(&mut svm, &user.pubkey(), &mint_y, 1_000_000);
    set_mint_supply(&mut svm, &mint_x, 1_000_000);
    set_mint_supply(&mut svm, &mint_y, 1_000_000);

    // fee = 10000 (exactly 100%) should succeed
    let ix = build_initialize_ix(
        &user.pubkey(), &mint_x, &mint_y, 99, &user.pubkey(), 10_000, 1_000_000, 1_000_000,
    );
    let tx = Transaction::new(
        &[&user],
        Message::new(&[ix], Some(&user.pubkey())),
        svm.latest_blockhash(),
    );
    assert!(svm.send_transaction(tx).is_ok(), "10000 bps (100%) should be valid");
}

// =============================================================================
// Helper
// =============================================================================

fn integer_sqrt(n: u128) -> u128 {
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
