use crate::common::*;
use solana_keypair::Keypair;
use solana_message::Message;
use solana_signer::Signer;
use solana_transaction::Transaction;
use spl_associated_token_account::get_associated_token_address;

const SEED: u64 = 77;
const FEE: u16 = 300;
const INIT_X: u64 = 100_000_000;
const INIT_Y: u64 = 100_000_000;
const CLOCK_TS: i64 = 1_000_000;

fn send_and_get_cu(svm: &mut litesvm::LiteSVM, payer: &Keypair, ix: solana_instruction::Instruction) -> u64 {
    let tx = Transaction::new(
        &[payer],
        Message::new(&[ix], Some(&payer.pubkey())),
        svm.latest_blockhash(),
    );
    let meta = svm.send_transaction(tx).expect("transaction failed");
    meta.compute_units_consumed
}

#[test]
fn bench_all_instructions_cu() {
    // --- Initialize ---
    let mut svm = setup_svm_with_clock(CLOCK_TS);
    let init_kp = Keypair::new();
    svm.airdrop(&init_kp.pubkey(), 100_000_000_000).unwrap();

    let mint_x = Keypair::new().pubkey();
    let mint_y = Keypair::new().pubkey();
    create_mint(&mut svm, &mint_x, &init_kp.pubkey(), 6);
    create_mint(&mut svm, &mint_y, &init_kp.pubkey(), 6);
    create_token_account(&mut svm, &init_kp.pubkey(), &mint_x, INIT_X);
    create_token_account(&mut svm, &init_kp.pubkey(), &mint_y, INIT_Y);
    set_mint_supply(&mut svm, &mint_x, INIT_X);
    set_mint_supply(&mut svm, &mint_y, INIT_Y);

    let ix = build_initialize_ix(
        &init_kp.pubkey(), &mint_x, &mint_y, SEED,
        &init_kp.pubkey(), FEE, INIT_X, INIT_Y,
    );
    let cu_init = send_and_get_cu(&mut svm, &init_kp, ix);

    let (config, _) = derive_config_pda(SEED, &mint_x, &mint_y);
    let (lp_mint, _) = derive_lp_mint_pda(&config);

    // --- Deposit ---
    let user = Keypair::new();
    svm.airdrop(&user.pubkey(), 10_000_000_000).unwrap();
    create_token_account(&mut svm, &user.pubkey(), &mint_x, 50_000_000);
    create_token_account(&mut svm, &user.pubkey(), &mint_y, 50_000_000);
    create_token_account(&mut svm, &user.pubkey(), &lp_mint, 0);

    let ix = build_deposit_ix(
        &user.pubkey(), &mint_x, &mint_y, &config,
        1_000, 50_000_000, 50_000_000, CLOCK_TS + 1000,
    );
    let cu_deposit = send_and_get_cu(&mut svm, &user, ix);

    // --- Swap ---
    let swapper = Keypair::new();
    svm.airdrop(&swapper.pubkey(), 10_000_000_000).unwrap();
    create_token_account(&mut svm, &swapper.pubkey(), &mint_x, 10_000_000);
    create_token_account(&mut svm, &swapper.pubkey(), &mint_y, 0);

    let ix = build_swap_ix(
        &swapper.pubkey(), &mint_x, &mint_y, &config,
        1_000_000, 1, CLOCK_TS + 1000,
    );
    let cu_swap = send_and_get_cu(&mut svm, &swapper, ix);

    // --- Withdraw ---
    // Give user some LP tokens to burn
    let user_lp_addr = get_associated_token_address(&user.pubkey(), &lp_mint);
    let user_lp_bal = get_token_balance(&svm, &user_lp_addr);
    assert!(user_lp_bal > 0, "User must have LP tokens to withdraw");

    svm.expire_blockhash();
    let ix = build_withdraw_ix(
        &user.pubkey(), &mint_x, &mint_y, &config,
        user_lp_bal, 1, 1, CLOCK_TS + 1000,
    );
    let cu_withdraw = send_and_get_cu(&mut svm, &user, ix);

    // --- UpdateFee ---
    let ix = build_update_fee_ix(&init_kp.pubkey(), &config, 500);
    let cu_update_fee = send_and_get_cu(&mut svm, &init_kp, ix);

    // --- UpdateAuthority ---
    svm.expire_blockhash();
    let new_auth = Keypair::new().pubkey();
    let ix = build_update_authority_ix(&init_kp.pubkey(), &config, &new_auth);
    let cu_update_auth = send_and_get_cu(&mut svm, &init_kp, ix);

    // --- UpdateLock (use new authority) ---
    // Reset authority back to init_kp first
    // Actually new_auth is now the authority but we don't have its keypair.
    // Let's just use a fresh pool for lock/remove tests.
    let mut ctx2 = PoolContext::new(88, FEE, INIT_X, INIT_Y, CLOCK_TS);

    let ix = build_update_lock_ix(&ctx2.initializer.pubkey(), &ctx2.config);
    let cu_update_lock = send_and_get_cu(&mut ctx2.svm, &ctx2.initializer, ix);

    // --- RemoveAuthority ---
    ctx2.svm.expire_blockhash();
    let ix = build_remove_authority_ix(&ctx2.initializer.pubkey(), &ctx2.config);
    let cu_remove_auth = send_and_get_cu(&mut ctx2.svm, &ctx2.initializer, ix);

    // --- Print results ---
    eprintln!("\n========== CU BENCHMARK ==========");
    eprintln!("Initialize:      {:>6} CU", cu_init);
    eprintln!("Deposit:         {:>6} CU", cu_deposit);
    eprintln!("Withdraw:        {:>6} CU", cu_withdraw);
    eprintln!("Swap:            {:>6} CU", cu_swap);
    eprintln!("UpdateFee:       {:>6} CU", cu_update_fee);
    eprintln!("UpdateAuthority: {:>6} CU", cu_update_auth);
    eprintln!("UpdateLock:      {:>6} CU", cu_update_lock);
    eprintln!("RemoveAuthority: {:>6} CU", cu_remove_auth);
    eprintln!("==================================\n");
}
