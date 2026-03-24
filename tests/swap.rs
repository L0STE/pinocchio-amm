use crate::common::*;
use solana_keypair::Keypair;
use solana_message::Message;
use solana_signer::Signer;
use solana_transaction::Transaction;
use spl_associated_token_account::get_associated_token_address;

const SEED: u64 = 3;
const FEE: u16 = 300; // 3%
const INIT_X: u64 = 100_000_000; // 100 tokens
const INIT_Y: u64 = 100_000_000;
const CLOCK_TS: i64 = 1_000_000;

fn setup_pool() -> PoolContext {
    PoolContext::new(SEED, FEE, INIT_X, INIT_Y, CLOCK_TS)
}

// =============================================================================
// HAPPY PATH
// =============================================================================

#[test]
fn test_swap_x_to_y_success() {
    let mut ctx = setup_pool();

    let user = ctx.add_user(10_000_000, 0);
    // User needs a token Y account to receive output
    create_token_account(&mut ctx.svm, &user.pubkey(), &ctx.mint_y, 0);

    // Record state before
    let vault_x = get_associated_token_address(&ctx.config, &ctx.mint_x);
    let vault_y = get_associated_token_address(&ctx.config, &ctx.mint_y);
    let vault_x_before = get_token_balance(&ctx.svm, &vault_x);
    let vault_y_before = get_token_balance(&ctx.svm, &vault_y);
    let user_x = get_associated_token_address(&user.pubkey(), &ctx.mint_x);
    let user_y = get_associated_token_address(&user.pubkey(), &ctx.mint_y);

    let swap_amount: u64 = 1_000_000; // 1 token

    let ix = build_swap_ix(
        &user.pubkey(),
        &ctx.mint_x, // from X
        &ctx.mint_y, // to Y
        &ctx.config,
        swap_amount,
        1, // min output
        CLOCK_TS + 1000,
    );

    let tx = Transaction::new(
        &[&user],
        Message::new(&[ix], Some(&user.pubkey())),
        ctx.svm.latest_blockhash(),
    );
    let result = ctx.svm.send_transaction(tx);
    assert!(result.is_ok(), "Swap X->Y failed: {:?}", result.err());

    // --- Verify user received Y tokens ---
    let user_y_after = get_token_balance(&ctx.svm, &user_y);
    assert!(user_y_after > 0, "User should receive Y tokens");

    // --- Verify user spent X tokens ---
    let user_x_after = get_token_balance(&ctx.svm, &user_x);
    assert!(user_x_after < 10_000_000, "User should spend X tokens");

    // --- Verify vault_x increased, vault_y decreased ---
    let vault_x_after = get_token_balance(&ctx.svm, &vault_x);
    let vault_y_after = get_token_balance(&ctx.svm, &vault_y);
    assert!(vault_x_after > vault_x_before, "Vault X should increase");
    assert!(vault_y_after < vault_y_before, "Vault Y should decrease");

    // --- Verify constant product maintained (k should increase due to fees) ---
    let k_before = vault_x_before as u128 * vault_y_before as u128;
    let k_after = vault_x_after as u128 * vault_y_after as u128;
    assert!(k_after >= k_before, "k should increase or stay same (fees)");
}

#[test]
fn test_swap_y_to_x_success() {
    let mut ctx = setup_pool();

    let user = ctx.add_user(0, 10_000_000);
    create_token_account(&mut ctx.svm, &user.pubkey(), &ctx.mint_x, 0);

    let swap_amount: u64 = 1_000_000;

    let ix = build_swap_ix(
        &user.pubkey(),
        &ctx.mint_y, // from Y
        &ctx.mint_x, // to X
        &ctx.config,
        swap_amount,
        1,
        CLOCK_TS + 1000,
    );

    let tx = Transaction::new(
        &[&user],
        Message::new(&[ix], Some(&user.pubkey())),
        ctx.svm.latest_blockhash(),
    );
    let result = ctx.svm.send_transaction(tx);
    assert!(result.is_ok(), "Swap Y->X failed: {:?}", result.err());

    let user_x = get_associated_token_address(&user.pubkey(), &ctx.mint_x);
    assert!(get_token_balance(&ctx.svm, &user_x) > 0, "User should receive X");
}

// =============================================================================
// ERROR PATHS
// =============================================================================

#[test]
fn test_swap_fails_with_zero_amount() {
    let mut ctx = setup_pool();
    let user = ctx.add_user(10_000_000, 0);
    create_token_account(&mut ctx.svm, &user.pubkey(), &ctx.mint_y, 0);

    let ix = build_swap_ix(
        &user.pubkey(), &ctx.mint_x, &ctx.mint_y, &ctx.config,
        0, 1, CLOCK_TS + 1000,
    );
    let tx = Transaction::new(
        &[&user],
        Message::new(&[ix], Some(&user.pubkey())),
        ctx.svm.latest_blockhash(),
    );
    assert!(ctx.svm.send_transaction(tx).is_err(), "Should fail with zero amount");
}

#[test]
fn test_swap_fails_with_slippage_exceeded() {
    let mut ctx = setup_pool();
    let user = ctx.add_user(1_000_000, 0);
    create_token_account(&mut ctx.svm, &user.pubkey(), &ctx.mint_y, 0);

    let ix = build_swap_ix(
        &user.pubkey(), &ctx.mint_x, &ctx.mint_y, &ctx.config,
        1_000_000,
        u64::MAX, // min = absurd, guaranteed slippage fail
        CLOCK_TS + 1000,
    );
    let tx = Transaction::new(
        &[&user],
        Message::new(&[ix], Some(&user.pubkey())),
        ctx.svm.latest_blockhash(),
    );
    assert!(ctx.svm.send_transaction(tx).is_err(), "Should fail with slippage exceeded");
}

#[test]
fn test_swap_fails_when_pool_locked() {
    let mut ctx = setup_pool();

    // Lock
    send_tx(
        &mut ctx.svm,
        &ctx.initializer,
        build_update_lock_ix(&ctx.initializer.pubkey(), &ctx.config),
    ).unwrap();

    let user = ctx.add_user(1_000_000, 0);
    create_token_account(&mut ctx.svm, &user.pubkey(), &ctx.mint_y, 0);

    let ix = build_swap_ix(
        &user.pubkey(), &ctx.mint_x, &ctx.mint_y, &ctx.config,
        1_000_000, 1, CLOCK_TS + 1000,
    );
    let tx = Transaction::new(
        &[&user],
        Message::new(&[ix], Some(&user.pubkey())),
        ctx.svm.latest_blockhash(),
    );
    assert!(ctx.svm.send_transaction(tx).is_err(), "Should fail when locked");
}

#[test]
fn test_swap_fails_with_expired_offer() {
    let mut ctx = setup_pool();
    let user = ctx.add_user(1_000_000, 0);
    create_token_account(&mut ctx.svm, &user.pubkey(), &ctx.mint_y, 0);

    let ix = build_swap_ix(
        &user.pubkey(), &ctx.mint_x, &ctx.mint_y, &ctx.config,
        1_000_000, 1,
        CLOCK_TS - 1, // expired
    );
    let tx = Transaction::new(
        &[&user],
        Message::new(&[ix], Some(&user.pubkey())),
        ctx.svm.latest_blockhash(),
    );
    assert!(ctx.svm.send_transaction(tx).is_err(), "Should fail with expired offer");
}

#[test]
fn test_swap_fails_with_invalid_mint_pair() {
    let mut ctx = setup_pool();

    // Create a random mint that's not in the pool
    let fake_mint = Keypair::new().pubkey();
    create_mint(&mut ctx.svm, &fake_mint, &ctx.initializer.pubkey(), 6);

    let user = ctx.add_user(1_000_000, 0);
    create_token_account(&mut ctx.svm, &user.pubkey(), &fake_mint, 0);

    // Try to swap from mint_x to fake_mint
    let ix = build_swap_ix(
        &user.pubkey(), &ctx.mint_x, &fake_mint, &ctx.config,
        1_000_000, 1, CLOCK_TS + 1000,
    );
    let tx = Transaction::new(
        &[&user],
        Message::new(&[ix], Some(&user.pubkey())),
        ctx.svm.latest_blockhash(),
    );
    assert!(ctx.svm.send_transaction(tx).is_err(), "Should fail with invalid mint pair");
}
