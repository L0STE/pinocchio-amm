use crate::common::*;
use solana_message::Message;
use solana_signer::Signer;
use solana_transaction::Transaction;
use spl_associated_token_account::get_associated_token_address;

const SEED: u64 = 2;
const FEE: u16 = 300;
const INIT_X: u64 = 10_000_000;
const INIT_Y: u64 = 20_000_000;
const CLOCK_TS: i64 = 1_000_000;

fn setup_pool() -> PoolContext {
    PoolContext::new(SEED, FEE, INIT_X, INIT_Y, CLOCK_TS)
}

// =============================================================================
// HAPPY PATH
// =============================================================================

#[test]
fn test_withdraw_success() {
    let mut ctx = setup_pool();

    // Initializer has LP tokens from initialization
    let initializer_lp = get_associated_token_address(&ctx.initializer.pubkey(), &ctx.lp_mint);
    let lp_balance = get_token_balance(&ctx.svm, &initializer_lp);
    assert!(lp_balance > 0, "Initializer should have LP tokens");

    // Get balances before
    let vault_x = get_associated_token_address(&ctx.config, &ctx.mint_x);
    let vault_y = get_associated_token_address(&ctx.config, &ctx.mint_y);
    let vault_x_before = get_token_balance(&ctx.svm, &vault_x);
    let vault_y_before = get_token_balance(&ctx.svm, &vault_y);
    let user_x = get_associated_token_address(&ctx.initializer.pubkey(), &ctx.mint_x);
    let user_y = get_associated_token_address(&ctx.initializer.pubkey(), &ctx.mint_y);
    let user_x_before = get_token_balance(&ctx.svm, &user_x);
    let user_y_before = get_token_balance(&ctx.svm, &user_y);

    // Withdraw half the LP tokens
    let withdraw_amount = lp_balance / 2;
    let ix = build_withdraw_ix(
        &ctx.initializer.pubkey(),
        &ctx.mint_x,
        &ctx.mint_y,
        &ctx.config,
        withdraw_amount,
        1, // min_x (low slippage tolerance)
        1, // min_y
        CLOCK_TS + 1000,
    );

    let tx = Transaction::new(
        &[&ctx.initializer],
        Message::new(&[ix], Some(&ctx.initializer.pubkey())),
        ctx.svm.latest_blockhash(),
    );
    let result = ctx.svm.send_transaction(tx);
    assert!(result.is_ok(), "Withdraw failed: {:?}", result.err());

    // --- Verify LP burned ---
    let lp_after = get_token_balance(&ctx.svm, &initializer_lp);
    assert_eq!(lp_after, lp_balance - withdraw_amount);

    // --- Verify tokens received ---
    let user_x_after = get_token_balance(&ctx.svm, &user_x);
    let user_y_after = get_token_balance(&ctx.svm, &user_y);
    assert!(user_x_after > user_x_before, "Should receive token X");
    assert!(user_y_after > user_y_before, "Should receive token Y");

    // --- Verify vault balances decreased ---
    let vault_x_after = get_token_balance(&ctx.svm, &vault_x);
    let vault_y_after = get_token_balance(&ctx.svm, &vault_y);
    assert!(vault_x_after < vault_x_before, "Vault X should decrease");
    assert!(vault_y_after < vault_y_before, "Vault Y should decrease");

    // --- Verify conservation: user received = vault lost ---
    assert_eq!(
        user_x_after - user_x_before,
        vault_x_before - vault_x_after,
    );
    assert_eq!(
        user_y_after - user_y_before,
        vault_y_before - vault_y_after,
    );
}

// =============================================================================
// ERROR PATHS
// =============================================================================

#[test]
fn test_withdraw_fails_with_zero_amount() {
    let mut ctx = setup_pool();

    let ix = build_withdraw_ix(
        &ctx.initializer.pubkey(), &ctx.mint_x, &ctx.mint_y, &ctx.config,
        0, 1, 1, CLOCK_TS + 1000,
    );
    let tx = Transaction::new(
        &[&ctx.initializer],
        Message::new(&[ix], Some(&ctx.initializer.pubkey())),
        ctx.svm.latest_blockhash(),
    );
    assert!(ctx.svm.send_transaction(tx).is_err(), "Should fail with zero amount");
}

#[test]
fn test_withdraw_fails_with_slippage_exceeded() {
    let mut ctx = setup_pool();

    // Withdraw 1 LP token but demand absurd minimums
    let ix = build_withdraw_ix(
        &ctx.initializer.pubkey(), &ctx.mint_x, &ctx.mint_y, &ctx.config,
        1,
        u64::MAX, // min_x = way more than possible
        u64::MAX, // min_y
        CLOCK_TS + 1000,
    );
    let tx = Transaction::new(
        &[&ctx.initializer],
        Message::new(&[ix], Some(&ctx.initializer.pubkey())),
        ctx.svm.latest_blockhash(),
    );
    assert!(ctx.svm.send_transaction(tx).is_err(), "Should fail with slippage exceeded");
}

#[test]
fn test_withdraw_fails_when_pool_locked() {
    let mut ctx = setup_pool();

    // Lock
    let lock_ix = build_update_lock_ix(&ctx.initializer.pubkey(), &ctx.config);
    send_tx(&mut ctx.svm, &ctx.initializer, lock_ix).unwrap();

    let ix = build_withdraw_ix(
        &ctx.initializer.pubkey(), &ctx.mint_x, &ctx.mint_y, &ctx.config,
        100, 1, 1, CLOCK_TS + 1000,
    );
    let tx = Transaction::new(
        &[&ctx.initializer],
        Message::new(&[ix], Some(&ctx.initializer.pubkey())),
        ctx.svm.latest_blockhash(),
    );
    assert!(ctx.svm.send_transaction(tx).is_err(), "Should fail when locked");
}

#[test]
fn test_withdraw_fails_with_expired_offer() {
    let mut ctx = setup_pool();

    let ix = build_withdraw_ix(
        &ctx.initializer.pubkey(), &ctx.mint_x, &ctx.mint_y, &ctx.config,
        100, 1, 1,
        CLOCK_TS - 1, // expired
    );
    let tx = Transaction::new(
        &[&ctx.initializer],
        Message::new(&[ix], Some(&ctx.initializer.pubkey())),
        ctx.svm.latest_blockhash(),
    );
    assert!(ctx.svm.send_transaction(tx).is_err(), "Should fail with expired offer");
}
