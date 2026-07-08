//! mollusk 0.13.4: the pinocchio "rent bug" does NOT reproduce.
//!
//! Somewhere in 0.13.1..=0.13.4 mollusk's default Rent sysvar moved to the
//! SIMD-0194 values `{ lamports_per_byte: 6960, exemption_threshold: 1.0 }`
//! (solana-rent 4.x defaults). pinocchio's `(128 + data_len) *
//! lamports_per_byte` now yields the full 2,039,280-lamport minimum, so the
//! exact same fixture that fails under mollusk 0.13.0 succeeds here.
//!
//! This matches mainnet: its rent sysvar (checked 2026-07-08 via RPC) already
//! holds `{6960, 1.0}`, so pinocchio computes the correct value in production.

#[cfg(test)]
mod tests {
    use mollusk_svm::Mollusk;
    use mollusk_svm_programs_token::token;
    use solana_account::Account;
    use solana_address::Address;
    use solana_instruction::{AccountMeta, Instruction};

    const PROGRAM_ID: Address = Address::new_from_array([1u8; 32]);

    const TOKEN_PROGRAM: Address = Address::new_from_array([
        6, 221, 246, 225, 215, 101, 161, 147, 217, 203, 225, 70, 206, 235, 121, 172,
        28, 180, 133, 237, 95, 91, 55, 145, 58, 140, 245, 133, 126, 255, 0, 169,
    ]);

    // 11111111111111111111111111111111
    const SYSTEM_PROGRAM: Address = Address::new_from_array([0u8; 32]);

    /// Full rent-exempt minimum for a 165-byte token account. Identical under
    /// both conventions: (128+165)*3480*2.0 == (128+165)*6960*1.0.
    const TOKEN_ACCOUNT_RENT: u64 = 2_039_280;

    fn mint_account() -> Account {
        let mut data = vec![0u8; 82];
        data[44] = 9;
        data[45] = 1;
        Account { lamports: 10_000, data, owner: TOKEN_PROGRAM.into(), ..Default::default() }
    }

    #[test]
    fn mollusk_0134_pinocchio_funds_full_rent_and_token_init_succeeds() {
        // fixture lives in the shared top-level fixtures/ directory
        std::env::set_var("SBF_OUT_DIR", concat!(env!("CARGO_MANIFEST_DIR"), "/../fixtures"));
        let mut mollusk = Mollusk::new(&PROGRAM_ID, "pinocchio_rent_bug");
        token::add_program(&mut mollusk);

        let payer = Address::new_unique();
        let mint = Address::new_unique();
        let owner = Address::new_unique();
        let (new_account, bump) = Address::find_program_address(&[b"token_account"], &PROGRAM_ID);

        let ix = Instruction {
            program_id: PROGRAM_ID,
            accounts: vec![
                AccountMeta::new(payer, true),               // [0] payer
                AccountMeta::new(new_account, false),        // [1] new account PDA
                AccountMeta::new_readonly(mint, false),      // [2] mint
                AccountMeta::new_readonly(owner, false),     // [3] owner
                AccountMeta::new_readonly(SYSTEM_PROGRAM, false), // [4] system program
                AccountMeta::new_readonly(TOKEN_PROGRAM, false),  // [5] token program
            ],
            data: vec![bump],
        };

        let (sys_key, sys_account) = mollusk_svm::program::keyed_account_for_system_program();
        let (tok_key, tok_account) = token::keyed_account();

        let accounts: &[(Address, Account)] = &[
            (payer, Account { lamports: 100_000_000_000, ..Default::default() }),
            (new_account, Account::default()),
            (mint, mint_account()),
            (owner, Account::default()),
            (sys_key, sys_account),
            (tok_key, tok_account),
        ];

        let result = mollusk.process_instruction(&ix, accounts);
        println!();
        println!("Result: {:?}", result.program_result);

        assert!(
            !result.program_result.is_err(),
            "Expected success under mollusk 0.13.4: its SIMD-0194 rent sysvar {{6960, 1.0}} \
             makes pinocchio's minimum_balance correct, got {:?}",
            result.program_result
        );

        let created = result
            .get_account(&new_account)
            .expect("new token account should exist");
        println!("new_account lamports: {}", created.lamports);
        assert_eq!(
            created.lamports, TOKEN_ACCOUNT_RENT,
            "pinocchio should fund the full rent-exempt minimum"
        );
        assert_eq!(created.data.len(), 165);
    }
}
