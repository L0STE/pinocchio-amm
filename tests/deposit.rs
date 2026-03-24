use crate::common::*;
use solana_message::Message;
use solana_signer::Signer;
use solana_transaction::Transaction;
use spl_associated_token_account::get_associated_token_address;

const SEED: u64 = 1;
const FEE: u16 = 300;
const INIT_X: u64 = 10_000_000; // 10 tokens
const INIT_Y: u64 = 20_000_000; // 20 tokens
const CLOCK_TS: i64 = 1_000_000;

fn setup_pool() -> PoolContext {
    PoolContext::new(SEED, FEE, INIT_X, INIT_Y, CLOCK_TS)
}

// =============================================================================
// HAPPY PATH
// =============================================================================

#[test]
fn test_deposit_success() {
    let mut ctx = setup_pool();

    let user = ctx.add_user(5_000_000, 10_000_000);
    // Create user LP account (empty)
    ctx.add_user_lp(&user.pubkey(), 0);

    // Get pool state before deposit
    let vault_x = get_associated_token_address(&ctx.config, &ctx.mint_x);
    let vault_y = get_associated_token_address(&ctx.config, &ctx.mint_y);
    let vault_x_before = get_token_balance(&ctx.svm, &vault_x);
    let vault_y_before = get_token_balance(&ctx.svm, &vault_y);
    let lp_supply_before = get_mint_supply(&ctx.svm, &ctx.lp_mint);

    // Deposit LP amount = 1000 (small relative to supply)
    let deposit_amount: u64 = 1_000;
    let ix = build_deposit_ix(
        &user.pubkey(),
        &ctx.mint_x,
        &ctx.mint_y,
        &ctx.config,
        deposit_amount,
        5_000_000, // max_x (generous slippage)
        10_000_000, // max_y
        CLOCK_TS + 1000, // future expiration
    );

    let tx = Transaction::new(
        &[&user],
        Message::new(&[ix], Some(&user.pubkey())),
        ctx.svm.latest_blockhash(),
    );
    let result = ctx.svm.send_transaction(tx);
    assert!(result.is_ok(), "Deposit failed: {:?}", result.err());

    // --- Verify LP tokens minted ---
    let user_lp = get_associated_token_address(&user.pubkey(), &ctx.lp_mint);
    assert_eq!(get_token_balance(&ctx.svm, &user_lp), deposit_amount);

    // --- Verify LP supply increased ---
    let lp_supply_after = get_mint_supply(&ctx.svm, &ctx.lp_mint);
    assert_eq!(lp_supply_after, lp_supply_before + deposit_amount);

    // --- Verify vault balances increased ---
    let vault_x_after = get_token_balance(&ctx.svm, &vault_x);
    let vault_y_after = get_token_balance(&ctx.svm, &vault_y);
    assert!(vault_x_after > vault_x_before, "Vault X should increase");
    assert!(vault_y_after > vault_y_before, "Vault Y should increase");
}

// =============================================================================
// ERROR PATHS
// =============================================================================

#[test]
fn test_deposit_fails_with_zero_amount() {
    let mut ctx = setup_pool();
    let user = ctx.add_user(5_000_000, 10_000_000);
    ctx.add_user_lp(&user.pubkey(), 0);

    let ix = build_deposit_ix(
        &user.pubkey(), &ctx.mint_x, &ctx.mint_y, &ctx.config,
        0, // zero amount
        5_000_000, 10_000_000, CLOCK_TS + 1000,
    );
    let tx = Transaction::new(
        &[&user],
        Message::new(&[ix], Some(&user.pubkey())),
        ctx.svm.latest_blockhash(),
    );
    assert!(ctx.svm.send_transaction(tx).is_err(), "Should fail with zero amount");
}

#[test]
fn test_deposit_fails_with_expired_offer() {
    let mut ctx = setup_pool();
    let user = ctx.add_user(5_000_000, 10_000_000);
    ctx.add_user_lp(&user.pubkey(), 0);

    let ix = build_deposit_ix(
        &user.pubkey(), &ctx.mint_x, &ctx.mint_y, &ctx.config,
        1_000, 5_000_000, 10_000_000,
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
fn test_deposit_fails_with_slippage_exceeded() {
    let mut ctx = setup_pool();
    let user = ctx.add_user(5_000_000, 10_000_000);
    ctx.add_user_lp(&user.pubkey(), 0);

    let ix = build_deposit_ix(
        &user.pubkey(), &ctx.mint_x, &ctx.mint_y, &ctx.config,
        1_000,
        1, // max_x = 1 (way too tight)
        1, // max_y = 1
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
fn test_deposit_fails_when_pool_locked() {
    let mut ctx = setup_pool();

    // Lock the pool
    let lock_ix = build_update_lock_ix(&ctx.initializer.pubkey(), &ctx.config);
    let tx = Transaction::new(
        &[&ctx.initializer],
        Message::new(&[lock_ix], Some(&ctx.initializer.pubkey())),
        ctx.svm.latest_blockhash(),
    );
    ctx.svm.send_transaction(tx).unwrap();

    let user = ctx.add_user(5_000_000, 10_000_000);
    ctx.add_user_lp(&user.pubkey(), 0);

    let ix = build_deposit_ix(
        &user.pubkey(), &ctx.mint_x, &ctx.mint_y, &ctx.config,
        1_000, 5_000_000, 10_000_000, CLOCK_TS + 1000,
    );
    let tx = Transaction::new(
        &[&user],
        Message::new(&[ix], Some(&user.pubkey())),
        ctx.svm.latest_blockhash(),
    );
    assert!(ctx.svm.send_transaction(tx).is_err(), "Should fail when pool locked");
}
