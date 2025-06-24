use crate::error::ErrorCode;
use crate::states::*;
use anchor_lang::prelude::*;

#[derive(Accounts)]
pub struct ClosePermissionPda<'info> {
    #[account(
        mut,
        address = crate::admin::ID @ ErrorCode::InvalidOwner
    )]
    pub owner: Signer<'info>,

    /// CHECK: permission account authority
    pub permission_authority: UncheckedAccount<'info>,

    /// Initialize config state account to store protocol owner address and fee rates.
    #[account(
        mut,
        seeds = [
            PERMISSION_SEED.as_bytes(),
            permission_authority.key().as_ref()
        ],
        bump,
        close = owner
    )]
    pub permission: Account<'info, Permission>,

    pub system_program: Program<'info, System>,
}

pub fn close_permission_pda(_ctx: Context<ClosePermissionPda>) -> Result<()> {
    Ok(())
}
