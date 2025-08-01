use crate::curve::CurveCalculator;
use crate::error::ErrorCode;
use crate::states::*;
use crate::utils::*;
use anchor_lang::system_program::{transfer, Transfer};
use anchor_lang::{
    accounts::interface_account::InterfaceAccount,
    prelude::*,
    solana_program::{self, clock, program::invoke, system_instruction},
};
use anchor_spl::token::{spl_token, Token};
use anchor_spl::token_2022::{
    self,
    spl_token_2022::{
        self,
        extension::{BaseStateWithExtensions, StateWithExtensions},
        instruction::AuthorityType,
    },
    Token2022,
};
use anchor_spl::token_2022_extensions::spl_token_metadata_interface;
use anchor_spl::{
    associated_token::{create, AssociatedToken, Create},
    token_interface::{Mint, TokenAccount},
};

#[derive(Accounts)]
pub struct InitializeV2<'info> {
    /// Address paying to create the pool. Can be anyone
    #[account(mut)]
    pub creator: Signer<'info>,

    /// Which config the pool belongs to.
    pub amm_config: Box<Account<'info, AmmConfig>>,

    /// CHECK: pool vault and position nft mint authority
    #[account(
        seeds = [
            crate::AUTH_SEED_V2.as_bytes(),
        ],
        bump,
    )]
    pub authority: UncheckedAccount<'info>,

    /// CHECK: Initialize an account to store the pool state
    /// PDA account:
    /// seeds = [
    ///     POOL_SEED.as_bytes(),
    ///     amm_config.key().as_ref(),
    ///     token_0_mint.key().as_ref(),
    ///     token_1_mint.key().as_ref(),
    /// ],
    ///
    /// Or random account: must be signed by cli
    #[account(mut)]
    pub pool_state: UncheckedAccount<'info>,

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
        payer = creator,
        space = Position::LEN
    )]
    pub position: Box<Account<'info, Position>>,

    /// Token_0 mint
    #[account(
        constraint = token_0_mint.key() < token_1_mint.key()
    )]
    pub token_0_mint: Box<InterfaceAccount<'info, Mint>>,

    /// Token_1 mint, the key must grater then token_0 mint.
    pub token_1_mint: Box<InterfaceAccount<'info, Mint>>,

    /// creator token0 account
    #[account(
        mut,
        token::mint = token_0_mint,
        token::authority = creator,
    )]
    pub creator_token_0: Box<InterfaceAccount<'info, TokenAccount>>,

    /// creator token1 account
    #[account(
        mut,
        token::mint = token_1_mint,
        token::authority = creator,
    )]
    pub creator_token_1: Box<InterfaceAccount<'info, TokenAccount>>,

    /// Token_0 vault for the pool
    #[account(
        init,
        seeds = [
            POOL_VAULT_SEED.as_bytes(),
            pool_state.key().as_ref(),
            token_0_mint.key().as_ref()
        ],
        bump,
        token::authority = authority,
        token::mint = token_0_mint,
        token::token_program = if token_0_mint.to_account_info().owner.key() == Token::id(){ token_program.to_account_info() } else { token_program_2022.to_account_info() },
        payer = creator,
    )]
    pub token_0_vault: Box<InterfaceAccount<'info, TokenAccount>>,

    /// Token_1 vault for the pool
    #[account(
        init,
        seeds = [
            POOL_VAULT_SEED.as_bytes(),
            pool_state.key().as_ref(),
            token_1_mint.key().as_ref()
        ],
        bump,
        token::authority = authority,
        token::mint = token_1_mint,
        token::token_program = if token_1_mint.to_account_info().owner.key() == Token::id(){ token_program.to_account_info() } else { token_program_2022.to_account_info() },
        payer = creator,
    )]
    pub token_1_vault: Box<InterfaceAccount<'info, TokenAccount>>,

    /// create pool fee account
    #[account(
        mut,
        address= crate::create_pool_fee_reveiver::ID,
    )]
    pub create_pool_fee: Box<InterfaceAccount<'info, TokenAccount>>,

    /// an account to store oracle observations
    #[account(
        init,
        seeds = [
            OBSERVATION_SEED.as_bytes(),
            pool_state.key().as_ref(),
        ],
        bump,
        payer = creator,
        space = ObservationState::LEN
    )]
    pub observation_state: AccountLoader<'info, ObservationState>,

    /// Token program
    pub token_program: Program<'info, Token>,
    /// Token program 2022
    pub token_program_2022: Program<'info, Token2022>,
    /// Program to create an ATA for receiving position NFT
    pub associated_token_program: Program<'info, AssociatedToken>,
    /// To create a new program account
    pub system_program: Program<'info, System>,
}

pub fn initialize_v2(
    ctx: Context<InitializeV2>,
    init_amount_0: u64,
    init_amount_1: u64,
    open_time: u64,
    fee_on: u8,
    with_metadata: bool,
) -> Result<()> {
    if !(is_supported_mint(&ctx.accounts.token_0_mint).unwrap()
        && is_supported_mint(&ctx.accounts.token_1_mint).unwrap())
    {
        return err!(ErrorCode::NotSupportMint);
    }

    if ctx.accounts.amm_config.disable_create_pool {
        return err!(ErrorCode::NotApproved);
    }
    let block_timestamp = clock::Clock::get()?.unix_timestamp as u64;
    let mut open_time = open_time;
    if open_time <= block_timestamp {
        open_time = block_timestamp + 1;
    }

    create_position_nft_mint_with_extensions(
        ctx.accounts.creator.to_account_info(),
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
            payer: ctx.accounts.creator.to_account_info(),
            associated_token: ctx.accounts.position_nft_account.to_account_info(),
            authority: ctx.accounts.position_nft_owner.to_account_info(),
            mint: ctx.accounts.position_nft_mint.to_account_info(),
            system_program: ctx.accounts.system_program.to_account_info(),
            token_program: ctx.accounts.token_program_2022.to_account_info(),
        },
    ))?;
    let pool_state_loader = crate::initialize::create_pool(
        &ctx.accounts.creator.to_account_info(),
        &ctx.accounts.pool_state.to_account_info(),
        &ctx.accounts.amm_config.to_account_info(),
        &ctx.accounts.token_0_mint.to_account_info(),
        &ctx.accounts.token_1_mint.to_account_info(),
        &ctx.accounts.system_program.to_account_info(),
    )?;
    let pool_state = &mut pool_state_loader.load_init()?;

    let mut observation_state = ctx.accounts.observation_state.load_init()?;
    observation_state.pool_id = ctx.accounts.pool_state.key();

    transfer_from_user_to_pool_vault(
        ctx.accounts.creator.to_account_info(),
        ctx.accounts.creator_token_0.to_account_info(),
        ctx.accounts.token_0_vault.to_account_info(),
        ctx.accounts.token_0_mint.to_account_info(),
        if ctx.accounts.token_0_mint.to_account_info().owner.key() == Token::id() {
            ctx.accounts.token_program.to_account_info()
        } else {
            ctx.accounts.token_program_2022.to_account_info()
        },
        init_amount_0,
        ctx.accounts.token_0_mint.decimals,
    )?;

    transfer_from_user_to_pool_vault(
        ctx.accounts.creator.to_account_info(),
        ctx.accounts.creator_token_1.to_account_info(),
        ctx.accounts.token_1_vault.to_account_info(),
        ctx.accounts.token_1_mint.to_account_info(),
        if ctx.accounts.token_1_mint.to_account_info().owner.key() == Token::id() {
            ctx.accounts.token_program.to_account_info()
        } else {
            ctx.accounts.token_program_2022.to_account_info()
        },
        init_amount_1,
        ctx.accounts.token_1_mint.decimals,
    )?;

    ctx.accounts.token_0_vault.reload()?;
    ctx.accounts.token_1_vault.reload()?;
    let token_0_vault_amount = ctx.accounts.token_0_vault.amount;
    let token_1_vault_amount = ctx.accounts.token_1_vault.amount;

    CurveCalculator::validate_supply(token_0_vault_amount, token_1_vault_amount)?;

    let liquidity = U128::from(token_0_vault_amount)
        .checked_mul(token_1_vault_amount.into())
        .unwrap()
        .integer_sqrt()
        .as_u64();
    let lock_lp_amount = 100;
    msg!(
        "liquidity:{}, lock_lp_amount:{}, vault_0_amount:{},vault_1_amount:{}",
        liquidity,
        lock_lp_amount,
        token_0_vault_amount,
        token_1_vault_amount
    );

    ctx.accounts.position.initialize(
        ctx.bumps.position,
        liquidity.checked_sub(lock_lp_amount).unwrap(),
        Clock::get()?.epoch,
        0,
        0,
        ctx.accounts.position_nft_mint.key(),
        ctx.accounts.pool_state.key(),
    )?;

    mint_nft_and_remove_mint_authority(
        ctx.accounts.creator.to_account_info(),
        ctx.accounts.authority.to_account_info(),
        ctx.accounts.position.to_account_info(),
        ctx.accounts.position_nft_mint.to_account_info(),
        ctx.accounts.position_nft_account.to_account_info(),
        ctx.accounts.token_program_2022.to_account_info(),
        with_metadata,
        &[&[crate::AUTH_SEED_V2.as_bytes(), &[ctx.bumps.authority]]],
    )?;

    // Charge the fee to create a pool
    if ctx.accounts.amm_config.create_pool_fee != 0 {
        invoke(
            &system_instruction::transfer(
                ctx.accounts.creator.key,
                &ctx.accounts.create_pool_fee.key(),
                u64::from(ctx.accounts.amm_config.create_pool_fee),
            ),
            &[
                ctx.accounts.creator.to_account_info(),
                ctx.accounts.create_pool_fee.to_account_info(),
                ctx.accounts.system_program.to_account_info(),
            ],
        )?;
        invoke(
            &spl_token::instruction::sync_native(
                ctx.accounts.token_program.key,
                &ctx.accounts.create_pool_fee.key(),
            )?,
            &[
                ctx.accounts.token_program.to_account_info(),
                ctx.accounts.create_pool_fee.to_account_info(),
            ],
        )?;
    }

    pool_state.initialize(
        ctx.bumps.authority,
        liquidity,
        open_time,
        ctx.accounts.creator.key(),
        ctx.accounts.amm_config.key(),
        ctx.accounts.token_0_vault.key(),
        ctx.accounts.token_1_vault.key(),
        &ctx.accounts.token_0_mint,
        &ctx.accounts.token_1_mint,
        Pubkey::default(),
        0,
        ctx.accounts.observation_state.key(),
        FeeOn::try_from(fee_on)?,
    );

    Ok(())
}

pub fn mint_nft_and_remove_mint_authority<'info>(
    payer: AccountInfo<'info>,
    authority: AccountInfo<'info>,
    position: AccountInfo<'info>,
    position_nft_mint: AccountInfo<'info>,
    position_nft_account: AccountInfo<'info>,
    token_program_2022: AccountInfo<'info>,
    with_metadata: bool,
    authority_signers_seeds: &[&[&[u8]]],
) -> Result<()> {
    if with_metadata {
        let (name, symbol, uri) = get_metadata_data(position.key());
        initialize_token_metadata_extension(
            payer,
            position_nft_mint.clone(),
            authority.clone(),
            position.clone(),
            token_program_2022.clone(),
            name,
            symbol,
            uri,
            authority_signers_seeds,
        )?;
    }
    // Mint the NFT
    token_2022::mint_to(
        CpiContext::new_with_signer(
            token_program_2022.clone(),
            token_2022::MintTo {
                mint: position_nft_mint.clone(),
                to: position_nft_account,
                authority: authority.clone(),
            },
            authority_signers_seeds,
        ),
        1,
    )?;

    // Disable minting
    token_2022::set_authority(
        CpiContext::new_with_signer(
            token_program_2022,
            token_2022::SetAuthority {
                current_authority: authority,
                account_or_mint: position_nft_mint,
            },
            authority_signers_seeds,
        ),
        AuthorityType::MintTokens,
        None,
    )
}

fn get_metadata_data(position_id: Pubkey) -> (String, String, String) {
    return (
        String::from("Raydium CPMM Liquidity"),
        String::from("RCP"),
        format!(
            "https://dynamic-ipfs.raydium.io/cpmm/position?id={}",
            position_id.to_string()
        ),
    );
}

pub fn initialize_token_metadata_extension<'info>(
    payer: AccountInfo<'info>,
    position_nft_mint: AccountInfo<'info>,
    mint_authority: AccountInfo<'info>,
    metadata_update_authority: AccountInfo<'info>,
    token_2022_program: AccountInfo<'info>,
    name: String,
    symbol: String,
    uri: String,
    signers_seeds: &[&[&[u8]]],
) -> Result<()> {
    let metadata = spl_token_metadata_interface::state::TokenMetadata {
        name,
        symbol,
        uri,
        ..Default::default()
    };

    let mint_data = position_nft_mint.try_borrow_data()?;
    let mint_state_unpacked =
        StateWithExtensions::<spl_token_2022::state::Mint>::unpack(&mint_data)?;
    let new_account_len =
        mint_state_unpacked.try_get_new_account_len_for_variable_len_extension(&metadata)?;
    let new_rent_exempt_lamports = Rent::get()?.minimum_balance(new_account_len);
    let additional_lamports = new_rent_exempt_lamports.saturating_sub(position_nft_mint.lamports());
    // CPI call will borrow the account data
    drop(mint_data);

    let cpi_context = CpiContext::new(
        token_2022_program.to_account_info(),
        Transfer {
            from: payer.to_account_info(),
            to: position_nft_mint.to_account_info(),
        },
    );
    transfer(cpi_context, additional_lamports)?;

    solana_program::program::invoke_signed(
        &spl_token_metadata_interface::instruction::initialize(
            token_2022_program.key,
            position_nft_mint.key,
            metadata_update_authority.key,
            position_nft_mint.key,
            &mint_authority.key(),
            metadata.name,
            metadata.symbol,
            metadata.uri,
        ),
        &[
            position_nft_mint.to_account_info(),
            mint_authority.to_account_info(),
            metadata_update_authority.to_account_info(),
            token_2022_program.to_account_info(),
        ],
        signers_seeds,
    )?;

    Ok(())
}
