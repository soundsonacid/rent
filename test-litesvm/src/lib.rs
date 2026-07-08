//! litesvm 0.10.0 counterpart of the Mollusk repro in `test/src/lib.rs`.
//!
//! Runs the same fixture (`pinocchio_rent_bug.so`) through litesvm to check
//! whether the pinocchio `Rent::minimum_balance` underfunding reproduces
//! under a different test harness, ruling out a Mollusk-specific sysvar
//! serialization issue.

#[cfg(test)]
mod tests {
    use litesvm::LiteSVM;
    use solana_account::Account;
    use solana_address::Address;
    use solana_instruction::{AccountMeta, Instruction};
    use solana_keypair::Keypair;
    use solana_rent::Rent;
    use solana_signer::Signer;
    use solana_transaction::Transaction;

    const PROGRAM_ID: Address = Address::new_from_array([1u8; 32]);

    // TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA — litesvm ships this
    // program built in, so the fixture's CPI target is already loaded.
    const TOKEN_PROGRAM: Address = Address::new_from_array([
        6, 221, 246, 225, 215, 101, 161, 147, 217, 203, 225, 70, 206, 235, 121, 172,
        28, 180, 133, 237, 95, 91, 55, 145, 58, 140, 245, 133, 126, 255, 0, 169,
    ]);

    // 11111111111111111111111111111111
    const SYSTEM_PROGRAM: Address = Address::new_from_array([0u8; 32]);

    fn mint_account(lamports: u64) -> Account {
        let mut data = vec![0u8; 82];
        data[44] = 9; // decimals
        data[45] = 1; // is_initialized
        Account { lamports, data, owner: TOKEN_PROGRAM.into(), ..Default::default() }
    }

    #[test]
    fn litesvm_pinocchio_rent_creates_account_with_half_required_lamports() {
        let mut svm = LiteSVM::new();
        svm.add_program_from_file(
            PROGRAM_ID,
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../test/tests/fixtures/pinocchio_rent_bug.so"
            ),
        )
        .expect("failed to load pinocchio_rent_bug.so fixture");

        let payer = Keypair::new();
        svm.airdrop(&payer.pubkey(), 100_000_000_000).unwrap();

        let mint = Address::new_unique();
        let owner = Address::new_unique();
        let (new_account, bump) = Address::find_program_address(&[b"token_account"], &PROGRAM_ID);

        let mint_lamports = svm.minimum_balance_for_rent_exemption(82);
        svm.set_account(mint, mint_account(mint_lamports)).unwrap();

        let ix = Instruction {
            program_id: PROGRAM_ID,
            accounts: vec![
                AccountMeta::new(payer.pubkey(), true),      // [0] payer
                AccountMeta::new(new_account, false),        // [1] new account PDA
                AccountMeta::new_readonly(mint, false),      // [2] mint
                AccountMeta::new_readonly(owner, false),     // [3] owner
                AccountMeta::new_readonly(SYSTEM_PROGRAM, false), // [4] system program
                AccountMeta::new_readonly(TOKEN_PROGRAM, false),  // [5] token program
            ],
            data: vec![bump],
        };

        let tx = Transaction::new_signed_with_payer(
            &[ix],
            Some(&payer.pubkey()),
            &[&payer],
            svm.latest_blockhash(),
        );
        let result = svm.send_transaction(tx);

        let rent = Rent::default();
        let correct = rent.minimum_balance(165);
        #[allow(deprecated)] // mirrors pinocchio's computation, which uses this field
        let pinocchio_val = (128 + 165u64) * rent.lamports_per_byte_year;
        println!();
        println!("SDK    Rent::minimum_balance(165) = {correct}");
        println!("Pinocchio minimum_balance(165)    = {pinocchio_val}");
        println!("Ratio: {:.1}x", correct as f64 / pinocchio_val as f64);

        match &result {
            Ok(meta) => {
                println!("Transaction SUCCEEDED — bug does NOT reproduce under litesvm");
                for log in &meta.logs {
                    println!("  {log}");
                }
                if let Some(acct) = svm.get_account(&new_account) {
                    println!("new_account lamports: {}", acct.lamports);
                }
            }
            Err(failed) => {
                println!("Transaction FAILED: {:?}", failed.err);
                for log in &failed.meta.logs {
                    println!("  {log}");
                }
            }
        }

        let failed = result.expect_err(
            "Expected failure: pinocchio creates accounts with half the required rent lamports \
             (if this test fails, litesvm does NOT reproduce the bug)",
        );

        // Confirm it is the same failure mode as under Mollusk: SPL Token
        // rejecting InitializeAccount3 for insufficient rent, not some other
        // harness-specific error.
        assert!(
            failed
                .meta
                .logs
                .iter()
                .any(|l| l.contains("rent-exempt")),
            "Transaction failed, but not with the rent-exemption error: {:?}",
            failed.err
        );
    }
}
