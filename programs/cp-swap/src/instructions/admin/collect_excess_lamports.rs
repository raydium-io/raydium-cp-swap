use crate::error::ErrorCode;
use crate::utils::token::*;
use anchor_lang::prelude::*;
use anchor_spl::token::Token;
use anchor_spl::token_interface::Token2022;
#[derive(Accounts)]
pub struct CollectExcessLamports<'info> {
    /// Only admin or collect_lamports can collect lamports
    #[account(
        mut, 
        constraint = (collect_lamports_wallet.key() == crate::collect_lamports::ID || collect_lamports_wallet.key() == crate::admin::ID) @ ErrorCode::InvalidOwner
    )]
    pub collect_lamports_wallet: Signer<'info>,

    /// CHECK: pool vault and lp mint authority
    #[account(
        seeds = [
            crate::AUTH_SEED.as_bytes(),
        ],
        bump,
    )]
    pub authority: UncheckedAccount<'info>,

    /// The SPL program to perform token transfers
    pub token_program: Program<'info, Token>,

    /// The SPL program 2022 to perform token transfers
    pub token_program_2022: Program<'info, Token2022>,
    // remaining account
    // It can be vaults, LP mints, or PDA accounts.
    // `..+M` `[writable]` M source lamports accounts.
}

pub fn collect_excess_lamports<'info>(
    ctx: Context<'info, CollectExcessLamports<'info>>,
) -> Result<()> {
    for source_lamports_account in ctx.remaining_accounts.into_iter() {
        if *source_lamports_account.owner == Token::id() {
            withdraw_excess_lamports(
                ctx.accounts.token_program.to_account_info(),
                source_lamports_account.to_account_info(),
                ctx.accounts.collect_lamports_wallet.to_account_info(),
                ctx.accounts.authority.to_account_info(),
                crate::AUTH_SEED.as_bytes(),
                ctx.bumps.authority,
            )?;
        } else if *source_lamports_account.owner == Token2022::id() {
            withdraw_excess_lamports(
                ctx.accounts.token_program_2022.to_account_info(),
                source_lamports_account.to_account_info(),
                ctx.accounts.collect_lamports_wallet.to_account_info(),
                ctx.accounts.authority.to_account_info(),
                crate::AUTH_SEED.as_bytes(),
                ctx.bumps.authority,
            )?;
        } else if *source_lamports_account.owner == crate::id() {
            let rent = Rent::get()?;
            let minimum_balance = rent.minimum_balance(source_lamports_account.data_len());
            let source_lamports = source_lamports_account.lamports();
            let excess_lamports = source_lamports
                .checked_sub(minimum_balance)
                .ok_or(ProgramError::InsufficientFunds)?;

            if excess_lamports == 0 {
                return Ok(());
            }

            {
                let mut source_lamports_ref = source_lamports_account.try_borrow_mut_lamports()?;

                **source_lamports_ref = source_lamports
                    .checked_sub(excess_lamports)
                    .ok_or(ProgramError::InsufficientFunds)?;
            }

            {
                let mut destination_lamports_ref = ctx
                    .accounts
                    .collect_lamports_wallet
                    .try_borrow_mut_lamports()?;

                **destination_lamports_ref = destination_lamports_ref
                    .checked_add(excess_lamports)
                    .ok_or(ProgramError::ArithmeticOverflow)?;
            }
        } else {
            continue;
        }
    }
    Ok(())
}
