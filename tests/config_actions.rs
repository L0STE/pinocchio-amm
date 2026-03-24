use crate::common::*;
use solana_keypair::Keypair;
use solana_message::Message;
use solana_signer::Signer;
use solana_transaction::Transaction;

const SEED: u64 = 10;
const FEE: u16 = 300;
const INIT_X: u64 = 10_000_000;
const INIT_Y: u64 = 10_000_000;
const CLOCK_TS: i64 = 1_000_000;

fn setup_pool() -> PoolContext {
    PoolContext::new(SEED, FEE, INIT_X, INIT_Y, CLOCK_TS)
}

// =============================================================================
// UPDATE AUTHORITY
// =============================================================================

#[test]
fn test_update_authority_success() {
    let mut ctx = setup_pool();

    let new_authority = Keypair::new();
    let ix = build_update_authority_ix(
        &ctx.initializer.pubkey(),
        &ctx.config,
        &new_authority.pubkey(),
    );
    send_tx(&mut ctx.svm, &ctx.initializer, ix).unwrap();

    let data = get_config_data(&ctx.svm, &ctx.config);
    assert_eq!(
        &data[CONFIG_AUTH_OFF..CONFIG_AUTH_OFF + 32],
        new_authority.pubkey().as_ref(),
    );
}

#[test]
fn test_update_authority_fails_with_wrong_signer() {
    let mut ctx = setup_pool();

    let wrong = Keypair::new();
    ctx.svm.airdrop(&wrong.pubkey(), 1_000_000_000).unwrap();

    let ix = build_update_authority_ix(&wrong.pubkey(), &ctx.config, &wrong.pubkey());
    let tx = Transaction::new(
        &[&wrong],
        Message::new(&[ix], Some(&wrong.pubkey())),
        ctx.svm.latest_blockhash(),
    );
    assert!(ctx.svm.send_transaction(tx).is_err(), "Wrong authority should fail");
}

// =============================================================================
// UPDATE FEE
// =============================================================================

#[test]
fn test_update_fee_success() {
    let mut ctx = setup_pool();

    let new_fee: u16 = 500;
    let ix = build_update_fee_ix(&ctx.initializer.pubkey(), &ctx.config, new_fee);
    send_tx(&mut ctx.svm, &ctx.initializer, ix).unwrap();

    let data = get_config_data(&ctx.svm, &ctx.config);
    assert_eq!(get_config_fee(&data), new_fee);
}

#[test]
fn test_update_fee_fails_with_excessive_fee() {
    let mut ctx = setup_pool();

    let ix = build_update_fee_ix(&ctx.initializer.pubkey(), &ctx.config, 10_001);
    let result = send_tx(&mut ctx.svm, &ctx.initializer, ix);
    assert!(result.is_err(), "Fee > 10000 should fail");
}

#[test]
fn test_update_fee_with_max_fee_succeeds() {
    let mut ctx = setup_pool();

    let ix = build_update_fee_ix(&ctx.initializer.pubkey(), &ctx.config, 10_000);
    let result = send_tx(&mut ctx.svm, &ctx.initializer, ix);
    assert!(result.is_ok(), "Fee = 10000 should succeed");

    let data = get_config_data(&ctx.svm, &ctx.config);
    assert_eq!(get_config_fee(&data), 10_000);
}

#[test]
fn test_update_fee_with_zero_succeeds() {
    let mut ctx = setup_pool();

    let ix = build_update_fee_ix(&ctx.initializer.pubkey(), &ctx.config, 0);
    let result = send_tx(&mut ctx.svm, &ctx.initializer, ix);
    assert!(result.is_ok(), "Fee = 0 should succeed");

    let data = get_config_data(&ctx.svm, &ctx.config);
    assert_eq!(get_config_fee(&data), 0);
}

#[test]
fn test_update_fee_fails_with_wrong_authority() {
    let mut ctx = setup_pool();

    let wrong = Keypair::new();
    ctx.svm.airdrop(&wrong.pubkey(), 1_000_000_000).unwrap();

    let ix = build_update_fee_ix(&wrong.pubkey(), &ctx.config, 100);
    let tx = Transaction::new(
        &[&wrong],
        Message::new(&[ix], Some(&wrong.pubkey())),
        ctx.svm.latest_blockhash(),
    );
    assert!(ctx.svm.send_transaction(tx).is_err());
}

// =============================================================================
// UPDATE LOCK (toggle)
// =============================================================================

#[test]
fn test_update_lock_toggle() {
    let mut ctx = setup_pool();

    // Initially unlocked
    let data = get_config_data(&ctx.svm, &ctx.config);
    assert!(!get_config_locked(&data));

    // Lock
    let ix = build_update_lock_ix(&ctx.initializer.pubkey(), &ctx.config);
    send_tx(&mut ctx.svm, &ctx.initializer, ix).unwrap();

    let data = get_config_data(&ctx.svm, &ctx.config);
    assert!(get_config_locked(&data), "Should be locked");

    // Expire blockhash so second tx is not rejected as duplicate
    ctx.svm.expire_blockhash();

    // Unlock
    let ix = build_update_lock_ix(&ctx.initializer.pubkey(), &ctx.config);
    send_tx(&mut ctx.svm, &ctx.initializer, ix).unwrap();

    let data = get_config_data(&ctx.svm, &ctx.config);
    assert!(!get_config_locked(&data), "Should be unlocked");
}

#[test]
fn test_update_lock_fails_with_wrong_authority() {
    let mut ctx = setup_pool();

    let wrong = Keypair::new();
    ctx.svm.airdrop(&wrong.pubkey(), 1_000_000_000).unwrap();

    let ix = build_update_lock_ix(&wrong.pubkey(), &ctx.config);
    let tx = Transaction::new(
        &[&wrong],
        Message::new(&[ix], Some(&wrong.pubkey())),
        ctx.svm.latest_blockhash(),
    );
    assert!(ctx.svm.send_transaction(tx).is_err());
}

// =============================================================================
// REMOVE AUTHORITY
// =============================================================================

#[test]
fn test_remove_authority_success() {
    let mut ctx = setup_pool();

    let ix = build_remove_authority_ix(&ctx.initializer.pubkey(), &ctx.config);
    send_tx(&mut ctx.svm, &ctx.initializer, ix).unwrap();

    let data = get_config_data(&ctx.svm, &ctx.config);
    assert_eq!(get_config_authority(&data), [0u8; 32], "Authority should be zeroed");
}

#[test]
fn test_remove_authority_prevents_further_updates() {
    let mut ctx = setup_pool();

    // Remove authority
    let ix = build_remove_authority_ix(&ctx.initializer.pubkey(), &ctx.config);
    send_tx(&mut ctx.svm, &ctx.initializer, ix).unwrap();

    // Now try to update fee — should fail (authority is zeroed, nobody matches)
    let ix = build_update_fee_ix(&ctx.initializer.pubkey(), &ctx.config, 100);
    let result = send_tx(&mut ctx.svm, &ctx.initializer, ix);
    assert!(result.is_err(), "Should fail after authority removed");
}
