#![no_std]
#![allow(unsafe_op_in_unsafe_fn)]
#![allow(unexpected_cfgs)]

//! Minimal reproduction of pinocchio Rent::minimum_balance bug.
//!
//! pinocchio's `Rent` struct reads only `lamports_per_byte_year` (8 bytes)
//! from the sysvar and computes:
//!
//!   minimum_balance = (128 + data_len) * lamports_per_byte_year
//!
//! But the Solana SDK's `Rent` struct computes:
//!
//!   minimum_balance = (128 + data_len) * lamports_per_byte_year * exemption_threshold
//!
//! where exemption_threshold defaults to 2.0. So pinocchio produces HALF the
//! correct rent-exempt balance, causing CreateAccount::with_minimum_balance to
//! create accounts that native programs reject as not rent-exempt.

use pinocchio::{
    AccountView, ProgramResult,
    cpi::{invoke_signed, Signer, Seed},
    entrypoint,
    error::ProgramError,
    instruction::{InstructionAccount, InstructionView},
    sysvars::Sysvar,
};
use solana_address::Address;

pub const PROGRAM_ID: Address = Address::new_from_array([1u8; 32]);

const TOKEN_PROGRAM_ID: Address = Address::new_from_array([
    6, 221, 246, 225, 215, 101, 161, 147, 217, 203, 225, 70, 206, 235, 121, 172,
    28, 180, 133, 237, 95, 91, 55, 145, 58, 140, 245, 133, 126, 255, 0, 169,
]);

const TOKEN_ACCOUNT_SIZE: u64 = 165;

pinocchio::program_entrypoint!(process_instruction);
pinocchio::no_allocator!();
pinocchio::nostd_panic_handler!();

fn process_instruction(
    _program_id: &Address,
    accounts: &mut [AccountView],
    data: &[u8],
) -> ProgramResult {
    // accounts:
    //   [0] payer (signer, writable)
    //   [1] new_account (PDA, writable)
    //   [2] mint
    //   [3] owner
    //   [4] system program (for CreateAccount CPI)
    //   [5] token program (for InitializeAccount3 CPI)
    if accounts.len() < 6 || data.is_empty() {
        return Err(ProgramError::NotEnoughAccountKeys);
    }

    // step 1: create account using pinocchio's with_minimum_balance.
    // BUG: this computes (128 + 165) * 3480 = 1,019,640 lamports
    // but the correct rent-exempt minimum is 2,039,280 (2x higher).
    let create_ix = pinocchio_system::instructions::CreateAccount::with_minimum_balance(
        &accounts[0], &accounts[1], TOKEN_ACCOUNT_SIZE, &TOKEN_PROGRAM_ID, None,
    )?;

    let bump = [data[0]];
    let seeds = [Seed::from(b"token_account" as &[u8]), Seed::from(bump.as_ref())];
    let signer = Signer::from(seeds.as_ref());
    create_ix.invoke_signed(&[signer])?;

    // step 2: initialize as token account — this FAILS because the account
    // has insufficient lamports for rent exemption per the native Rent check.
    let mut ix_data = [0u8; 33];
    ix_data[0] = 18; // InitializeAccount3
    ix_data[1..33].copy_from_slice(accounts[3].address().as_ref());

    let ix_accounts = [
        InstructionAccount::writable(accounts[1].address()),
        InstructionAccount::readonly(accounts[2].address()),
    ];

    let instruction = InstructionView {
        program_id: &TOKEN_PROGRAM_ID,
        accounts: &ix_accounts,
        data: &ix_data,
    };

    invoke_signed(&instruction, &[&accounts[1], &accounts[2]], &[])
}
