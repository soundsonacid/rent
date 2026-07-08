//! litesvm 0.10.0: the pinocchio "rent bug" REPRODUCES.
//!
//! litesvm 0.10.0 (solana-rent 3.x defaults) installs the pre-SIMD-0194 Rent
//! sysvar `{ lamports_per_byte_year: 3480, exemption_threshold: 2.0 }`.
//! pinocchio ignores the threshold, funds the account with half the required
//! lamports, and SPL Token 3.5.0 rejects InitializeAccount3 with
//! "Lamport balance below rent-exempt threshold". litesvm's transaction-level
//! rent check reports it as InsufficientFundsForRent as well.
//!
//! See `test-litesvm-0110` for the same fixture passing under litesvm 0.11.0,
//! whose SIMD-0194 rent sysvar matches mainnet.

#[cfg(test)]
mod tests {
    use litesvm::LiteSVM;
    use solana_account::Account;
    use solana_address::Address;
    use solana_instruction::{AccountMeta, Instruction};
    use solana_keypair::Keypair;
    use solana_signer::Signer;
    use solana_transaction::Transaction;

    const PROGRAM_ID: Address = Address::new_from_array([1u8; 32]);

    // TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA — litesvm 0.10 ships
    // spl-token 3.5.0 built in at this address.
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
    fn litesvm_0100_pinocchio_funds_half_rent_and_token_init_fails() {
        let mut svm = LiteSVM::new();
        svm.add_program_from_file(
            PROGRAM_ID,
            concat!(env!("CARGO_MANIFEST_DIR"), "/../fixtures/pinocchio_rent_bug.so"),
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

        match &result {
            Ok(meta) => {
                println!("Transaction SUCCEEDED (unexpected under litesvm 0.10)");
                for log in &meta.logs {
                    println!("  {log}");
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
            "Expected failure under litesvm 0.10.0: its pre-SIMD-0194 rent sysvar \
             makes pinocchio fund half the required lamports",
        );

        // Confirm the failure mode is the rent check, not something else.
        assert!(
            failed.meta.logs.iter().any(|l| l.contains("rent-exempt")),
            "Transaction failed, but not with the rent-exemption error: {:?}",
            failed.err
        );
    }
}
