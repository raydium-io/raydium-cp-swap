use crate::states::*;
use anchor_lang::{
    prelude::*,
    solana_program::{program::invoke, system_instruction},
};
use anchor_spl::metadata::mpl_token_metadata::types::DataV2;
use anchor_spl::metadata::{self, Metadata};
use anchor_spl::token::{spl_token, Token};
use anchor_spl::token_interface::{Mint, TokenAccount};

/// Fee charged to create LP token metadata, 0.1 SOL, paid to the Raydium fee receiver.
pub const CREATE_LP_METADATA_FEE: u64 = 100_000_000;

#[derive(Accounts)]
pub struct CreateLpMetadata<'info> {
    /// Pays for the new accounts and the create fee. Can be anyone (permissionless).
    #[account(mut)]
    pub payer: Signer<'info>,

    /// Pool state, used to derive the lp mint and the metadata state PDA.
    pub pool_state: AccountLoader<'info, PoolState>,

    /// The LP mint whose metadata is being created.
    #[account(
        address = pool_state.load()?.lp_mint,
    )]
    pub lp_mint: Box<InterfaceAccount<'info, Mint>>,

    /// Metadata state account. First-come-first-serve: only the first caller can init it.
    #[account(
        init,
        seeds = [
            LP_METADATA_STATE_SEED.as_bytes(),
            pool_state.key().as_ref(),
        ],
        bump,
        payer = payer,
        space = LpMetadataState::LEN,
    )]
    pub lp_metadata_state: Box<Account<'info, LpMetadataState>>,

    /// CHECK: LP mint and metadata authority, signs the metadata CPI.
    #[account(
        seeds = [
            crate::AUTH_SEED.as_bytes(),
        ],
        bump,
    )]
    pub authority: UncheckedAccount<'info>,

    /// Metaplex metadata account for the LP mint.
    /// CHECK: created and owned by the metadata program.
    #[account(
        mut,
        seeds = [
            "metadata".as_bytes(),
            anchor_spl::metadata::ID.as_ref(),
            lp_mint.key().as_ref(),
        ],
        bump,
        seeds::program = anchor_spl::metadata::ID,
    )]
    pub metadata: UncheckedAccount<'info>,

    /// Raydium fee receiver, receives the 0.1 SOL create fee.
    #[account(
        mut,
        address = crate::create_pool_fee_reveiver::ID,
    )]
    pub create_pool_fee_reveiver: Box<InterfaceAccount<'info, TokenAccount>>,

    /// SPL token program, for syncing the wrapped SOL fee receiver.
    pub token_program: Program<'info, Token>,
    /// Metaplex token metadata program.
    pub metadata_program: Program<'info, Metadata>,
    /// System program.
    pub system_program: Program<'info, System>,
    /// Rent sysvar.
    pub rent: Sysvar<'info, Rent>,
}

pub fn create_lp_metadata(
    ctx: Context<CreateLpMetadata>,
    name: String,
    symbol: String,
    uri: String,
) -> Result<()> {
    // Store the creator so only they can update the metadata afterwards.
    let lp_metadata_state = &mut ctx.accounts.lp_metadata_state;
    lp_metadata_state.bump = ctx.bumps.lp_metadata_state;
    lp_metadata_state.creator = ctx.accounts.payer.key();

    // Charge 0.1 SOL to the Raydium fee receiver.
    invoke(
        &system_instruction::transfer(
            ctx.accounts.payer.key,
            &ctx.accounts.create_pool_fee_reveiver.key(),
            CREATE_LP_METADATA_FEE,
        ),
        &[
            ctx.accounts.payer.to_account_info(),
            ctx.accounts.create_pool_fee_reveiver.to_account_info(),
            ctx.accounts.system_program.to_account_info(),
        ],
    )?;
    invoke(
        &spl_token::instruction::sync_native(
            ctx.accounts.token_program.key,
            &ctx.accounts.create_pool_fee_reveiver.key(),
        )?,
        &[
            ctx.accounts.token_program.to_account_info(),
            ctx.accounts.create_pool_fee_reveiver.to_account_info(),
        ],
    )?;

    let data = DataV2 {
        name,
        symbol,
        uri,
        seller_fee_basis_points: 0,
        creators: None,
        collection: None,
        uses: None,
    };

    let signer_seeds: &[&[u8]] = &[crate::AUTH_SEED.as_bytes(), &[ctx.bumps.authority]];

    metadata::create_metadata_accounts_v3(
        CpiContext::new(
            ctx.accounts.metadata_program.to_account_info(),
            metadata::CreateMetadataAccountsV3 {
                metadata: ctx.accounts.metadata.to_account_info(),
                mint: ctx.accounts.lp_mint.to_account_info(),
                mint_authority: ctx.accounts.authority.to_account_info(),
                payer: ctx.accounts.payer.to_account_info(),
                update_authority: ctx.accounts.authority.to_account_info(),
                system_program: ctx.accounts.system_program.to_account_info(),
                rent: ctx.accounts.rent.to_account_info(),
            },
        )
        .with_signer(&[signer_seeds]),
        data,
        true,  // is_mutable
        false, // update_authority_is_signer (update_authority == mint_authority)
        None,  // collection_details
    )?;

    Ok(())
}
