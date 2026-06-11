use crate::error::ErrorCode;
use crate::states::*;
use anchor_lang::prelude::*;
use anchor_spl::{token_2022, token_interface::Mint};

#[derive(Accounts)]
pub struct CloseSupportMintAssociated<'info> {
    /// Address to be set as protocol owner.
    #[account(
        mut,
        constraint = (owner.key() == crate::admin::ID || owner.key() == crate::create_support_mint_associated_owner::ID) @ ErrorCode::NotApproved
    )]
    pub owner: Signer<'info>,
    /// Support token mint
    #[account(
        owner = token_2022::ID @ ErrorCode::NotApproved
    )]
    pub token_mint: InterfaceAccount<'info, Mint>,
    /// Initialize support mint state account to store support mint address and bump.
    #[account(
        mut,
        seeds = [
            SUPPORT_MINT_SEED.as_bytes(),
            token_mint.key().as_ref(),
        ],
        bump,
        close = owner,
    )]
    pub support_mint_associated: Account<'info, SupportMintAssociated>,

    pub system_program: Program<'info, System>,
}

pub fn close_support_mint_associated(_ctx: Context<CloseSupportMintAssociated>) -> Result<()> {
    Ok(())
}
