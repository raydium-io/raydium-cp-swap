use crate::error::ErrorCode;
use crate::states::*;
use anchor_lang::prelude::*;
use anchor_spl::metadata::mpl_token_metadata::types::DataV2;
use anchor_spl::metadata::{self, Metadata};
use anchor_spl::token_interface::Mint;

#[derive(Accounts)]
pub struct UpdateLpMetadata<'info> {
    /// The creator, the pool creator, or the protocol admin — any may update.
    pub updater: Signer<'info>,

    /// Pool state, used to derive the lp mint and the metadata state PDA.
    pub pool_state: AccountLoader<'info, PoolState>,

    /// Amm config account storing the protocol owner.
    #[account(address = pool_state.load()?.amm_config)]
    pub amm_config: Account<'info, AmmConfig>,

    /// The LP mint whose metadata is being updated.
    #[account(
        address = pool_state.load()?.lp_mint,
    )]
    pub lp_mint: Box<InterfaceAccount<'info, Mint>>,

    /// Metadata state account storing the creator.
    #[account(
        seeds = [
            LP_METADATA_STATE_SEED.as_bytes(),
            pool_state.key().as_ref(),
        ],
        bump,
    )]
    pub lp_metadata_state: Account<'info, LpMetadataState>,

    /// CHECK: LP mint and metadata authority, signs the metadata CPI.
    #[account(
        seeds = [
            crate::AUTH_SEED.as_bytes(),
        ],
        bump,
    )]
    pub authority: UncheckedAccount<'info>,

    /// Metaplex metadata account for the LP mint.
    /// CHECK: owned by the metadata program.
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

    /// Metaplex token metadata program.
    pub metadata_program: Program<'info, Metadata>,
}

pub fn update_lp_metadata(
    ctx: Context<UpdateLpMetadata>,
    name: String,
    symbol: String,
    uri: String,
) -> Result<()> {
    // The creator, the pool creator, or the protocol admin may update it.
    let updater = ctx.accounts.updater.key();
    let is_creator = updater == ctx.accounts.lp_metadata_state.creator;
    let is_pool_creator = updater == ctx.accounts.pool_state.load()?.pool_creator;
    let is_admin =
        updater == ctx.accounts.amm_config.protocol_owner || updater == crate::admin::ID;
    require!(
        is_creator || is_pool_creator || is_admin,
        ErrorCode::NotLpMetadataCreator
    );

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

    metadata::update_metadata_accounts_v2(
        CpiContext::new(
            ctx.accounts.metadata_program.to_account_info(),
            metadata::UpdateMetadataAccountsV2 {
                metadata: ctx.accounts.metadata.to_account_info(),
                update_authority: ctx.accounts.authority.to_account_info(),
            },
        )
        .with_signer(&[signer_seeds]),
        None,       // new_update_authority
        Some(data), // data
        None,       // primary_sale_happened
        None,       // is_mutable
    )?;

    Ok(())
}
