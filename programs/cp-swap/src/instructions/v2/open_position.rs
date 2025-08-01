use crate::states::*;
use crate::utils::token::*;
use anchor_lang::prelude::*;
use anchor_spl::associated_token::{create, AssociatedToken, Create};
use anchor_spl::token_interface::Token2022;

#[derive(Accounts)]
pub struct OpenPosition<'info> {
    /// Pays to create the position
    #[account(mut)]
    pub payer: Signer<'info>,

    /// CHECK: pool vault and position nft mint authority
    #[account(
        seeds = [
            crate::AUTH_SEED_V2.as_bytes(),
        ],
        bump,
    )]
    pub authority: UncheckedAccount<'info>,

    pub pool_state: AccountLoader<'info, PoolState>,

    /// CHECK: Receives the position NFT
    pub position_nft_owner: UncheckedAccount<'info>,

    /// Unique token mint address, initialize in contract
    #[account(mut)]
    pub position_nft_mint: Signer<'info>,

    /// CHECK: ATA address where position NFT will be minted, initialize in contract
    #[account(mut)]
    pub position_nft_account: UncheckedAccount<'info>,

    /// Position account
    #[account(
        init,
        seeds = [
            POSITION_SEED.as_bytes(),
            position_nft_mint.key().as_ref()
        ],
        bump,
        payer = payer,
        space = Position::LEN
    )]
    pub position: Account<'info, Position>,

    /// Token program 2022
    pub token_program_2022: Program<'info, Token2022>,
    /// Program to create an ATA for receiving position NFT
    pub associated_token_program: Program<'info, AssociatedToken>,
    /// To create a new program account
    pub system_program: Program<'info, System>,
}

pub fn open_position(ctx: Context<OpenPosition>, with_metadata: bool) -> Result<()> {
    create_position_nft_mint_with_extensions(
        ctx.accounts.payer.to_account_info(),
        ctx.accounts.position_nft_mint.to_account_info(),
        ctx.accounts.authority.to_account_info(),
        ctx.accounts.position.to_account_info(),
        ctx.accounts.system_program.to_account_info(),
        ctx.accounts.token_program_2022.to_account_info(),
        with_metadata,
    )?;

    // create user position nft account
    create(CpiContext::new(
        ctx.accounts.associated_token_program.to_account_info(),
        Create {
            payer: ctx.accounts.payer.to_account_info(),
            associated_token: ctx.accounts.position_nft_account.to_account_info(),
            authority: ctx.accounts.position_nft_owner.to_account_info(),
            mint: ctx.accounts.position_nft_mint.to_account_info(),
            system_program: ctx.accounts.system_program.to_account_info(),
            token_program: ctx.accounts.token_program_2022.to_account_info(),
        },
    ))?;

    let pool_state = ctx.accounts.pool_state.load()?;
    super::mint_nft_and_remove_mint_authority(
        ctx.accounts.payer.to_account_info(),
        ctx.accounts.authority.to_account_info(),
        ctx.accounts.position.to_account_info(),
        ctx.accounts.position_nft_mint.to_account_info(),
        ctx.accounts.position_nft_account.to_account_info(),
        ctx.accounts.token_program_2022.to_account_info(),
        with_metadata,
        &[&[crate::AUTH_SEED_V2.as_bytes(), &[ctx.bumps.authority]]],
    )?;

    ctx.accounts.position.initialize(
        ctx.bumps.position,
        0,
        Clock::get()?.epoch,
        pool_state.fees_token_0_per_lp,
        pool_state.fees_token_1_per_lp,
        ctx.accounts.position_nft_mint.key(),
        ctx.accounts.pool_state.key(),
    )?;
    Ok(())
}
