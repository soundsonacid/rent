//! mollusk 0.13.0: the pinocchio "rent bug" REPRODUCES.
//!
//! mollusk 0.13.0's default Rent sysvar is still the pre-SIMD-0194
//! `{ lamports_per_byte_year: 3480, exemption_threshold: 2.0 }`. pinocchio's
//! `Rent::minimum_balance` ignores the threshold and computes
//! `(128 + data_len) * lamports_per_byte`, so it funds the new account with
//! HALF the required lamports and the token program rejects
//! InitializeAccount3 with "Lamport balance below rent-exempt threshold".
//!
//! Mainnet's rent sysvar (checked 2026-07-08 via RPC) already holds the
//! SIMD-0194 values `{6960, 1.0}`, under which pinocchio's math is correct —
//! see `test-mollusk-0134`, where the same fixture passes.

#[cfg(test)]
mod tests {
    use mollusk_svm::Mollusk;
    use mollusk_svm_programs_token::token;
    use solana_account::Account;
    use solana_address::Address;
    use solana_instruction::{AccountMeta, Instruction};
    use solana_sdk::rent::Rent;

    const PROGRAM_ID: Address = Address::new_from_array([1u8; 32]);

    const TOKEN_PROGRAM: Address = Address::new_from_array([
        6, 221, 246, 225, 215, 101, 161, 147, 217, 203, 225, 70, 206, 235, 121, 172,
        28, 180, 133, 237, 95, 91, 55, 145, 58, 140, 245, 133, 126, 255, 0, 169,
    ]);

    // 11111111111111111111111111111111
    const SYSTEM_PROGRAM: Address = Address::new_from_array([0u8; 32]);

    fn mint_account() -> Account {
        let mut data = vec![0u8; 82];
        data[44] = 9;
        data[45] = 1;
        Account { lamports: 10_000, data, owner: TOKEN_PROGRAM.into(), ..Default::default() }
    }

    #[test]
    fn mollusk_0130_pinocchio_funds_half_rent_and_token_init_fails() {
        // fixture lives in the shared top-level fixtures/ directory
        std::env::set_var("SBF_OUT_DIR", concat!(env!("CARGO_MANIFEST_DIR"), "/../fixtures"));
        let mut mollusk = Mollusk::new(&PROGRAM_ID, "pinocchio_rent_bug");
        token::add_program(&mut mollusk);

        let payer = Address::new_unique();
        let mint = Address::new_unique();
        let owner = Address::new_unique();
        let (new_account, bump) = Address::find_program_address(&[b"token_account"], &PROGRAM_ID);

        // instruction accounts must include system program and token program
        // so the CPI can resolve them
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

        // solana-sdk 2.x Rent::default() matches mollusk 0.13.0's sysvar:
        // the OLD convention {3480, 2.0}.
        let rent = Rent::default();
        let full = rent.minimum_balance(165);
        let pinocchio_val = (128 + 165u64) * rent.lamports_per_byte_year;
        println!();
        println!(
            "harness rent sysvar (old): lamports_per_byte_year={} threshold={}",
            rent.lamports_per_byte_year, rent.exemption_threshold
        );
        println!("full rent-exempt minimum(165)  = {full}");
        println!("pinocchio-funded amount        = {pinocchio_val}");
        println!("Result: {:?}", result.program_result);

        assert!(
            result.program_result.is_err(),
            "Expected failure under mollusk 0.13.0: its pre-SIMD-0194 rent sysvar \
             makes pinocchio fund half the required lamports"
        );
    }
}
