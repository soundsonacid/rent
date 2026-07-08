//! litesvm 0.11.0: the pinocchio "rent bug" does NOT reproduce.
//!
//! litesvm 0.11.0 (solana-rent 4.x defaults) installs the SIMD-0194 Rent
//! sysvar `{ lamports_per_byte: 6960, exemption_threshold: 1.0 }`, and swaps
//! SPL Token for p-token at the Tokenkeg address (feature
//! `replace_spl_token_with_p_token`, active under FeatureSet::all_enabled).
//! pinocchio's `(128 + len) * lamports_per_byte` now yields the full
//! 2,039,280-lamport minimum, so the same fixture that fails under litesvm
//! 0.10.0 succeeds here.
//!
//! This matches mainnet: its rent sysvar (checked 2026-07-08 via RPC) already
//! holds `{6960, 1.0}`, so pinocchio computes the correct value in production.

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

    // TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA — p-token in litesvm 0.11.
    const TOKEN_PROGRAM: Address = Address::new_from_array([
        6, 221, 246, 225, 215, 101, 161, 147, 217, 203, 225, 70, 206, 235, 121, 172,
        28, 180, 133, 237, 95, 91, 55, 145, 58, 140, 245, 133, 126, 255, 0, 169,
    ]);

    // 11111111111111111111111111111111
    const SYSTEM_PROGRAM: Address = Address::new_from_array([0u8; 32]);

    /// Full rent-exempt minimum for a 165-byte token account. Identical under
    /// both rent conventions: (128+165)*3480*2.0 == (128+165)*6960*1.0.
    const TOKEN_ACCOUNT_RENT: u64 = 2_039_280;

    fn mint_account(lamports: u64) -> Account {
        let mut data = vec![0u8; 82];
        data[44] = 9; // decimals
        data[45] = 1; // is_initialized
        Account { lamports, data, owner: TOKEN_PROGRAM.into(), ..Default::default() }
    }

    #[test]
    fn litesvm_0110_pinocchio_funds_full_rent_and_token_init_succeeds() {
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
                println!("Transaction SUCCEEDED");
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

        result.expect("expected success under litesvm 0.11 (SIMD-0194 rent sysvar)");

        let created = svm
            .get_account(&new_account)
            .expect("new token account should exist");
        println!("new_account lamports: {}", created.lamports);
        assert_eq!(
            created.lamports, TOKEN_ACCOUNT_RENT,
            "pinocchio should fund the full rent-exempt minimum under SIMD-0194 rent values"
        );
        assert_eq!(created.owner, TOKEN_PROGRAM.into());
        assert_eq!(created.data.len(), 165);
    }
}
