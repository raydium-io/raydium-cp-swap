use crate::curve::CurveCalculator;
use crate::error::ErrorCode;
use crate::states::*;
use crate::utils::*;
use anchor_lang::{
    accounts::interface_account::InterfaceAccount,
    prelude::*,
    solana_program::{clock, program::invoke, system_instruction},
    system_program,
};
use anchor_spl::token_2022::spl_token_2022;
use anchor_spl::{
    associated_token::AssociatedToken,
    token::spl_token,
    token::Token,
    token_interface::{Mint, TokenAccount, TokenInterface},
};
use std::ops::Deref;

#[derive(Accounts)]
pub struct Initialize<'info> {
    /// Address paying to create the pool. Can be anyone
    #[account(mut)]
    pub creator: Signer<'info>,

    /// Which config the pool belongs to.
    pub amm_config: Box<Account<'info, AmmConfig>>,

    /// CHECK:
    /// pool vault and lp mint authority
    #[account(
        seeds = [
            crate::AUTH_SEED.as_bytes(),
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

    /// Token_0 mint, the key must smaller than token_1 mint.
    #[account(
        constraint = token_0_mint.key() < token_1_mint.key(),
        mint::token_program = token_0_program,
    )]
    pub token_0_mint: Box<InterfaceAccount<'info, Mint>>,

    /// Token_1 mint, the key must grater then token_0 mint.
    #[account(
        mint::token_program = token_1_program,
    )]
    pub token_1_mint: Box<InterfaceAccount<'info, Mint>>,

    /// pool lp mint
    #[account(
        init,
        seeds = [
            POOL_LP_MINT_SEED.as_bytes(),
            pool_state.key().as_ref(),
        ],
        bump,
        mint::decimals = 9,
        mint::authority = authority,
        payer = creator,
        mint::token_program = token_program,
    )]
    pub lp_mint: Box<InterfaceAccount<'info, Mint>>,

    /// payer token0 account
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

    /// creator lp token account
    #[account(
        init,
        associated_token::mint = lp_mint,
        associated_token::authority = creator,
        payer = creator,
        token::token_program = token_program,
    )]
    pub creator_lp_token: Box<InterfaceAccount<'info, TokenAccount>>,

    /// CHECK: Token_0 vault for the pool, created by contract
    #[account(
        mut,
        seeds = [
            POOL_VAULT_SEED.as_bytes(),
            pool_state.key().as_ref(),
            token_0_mint.key().as_ref()
        ],
        bump,
    )]
    pub token_0_vault: UncheckedAccount<'info>,

    /// CHECK: Token_1 vault for the pool, created by contract
    #[account(
        mut,
        seeds = [
            POOL_VAULT_SEED.as_bytes(),
            pool_state.key().as_ref(),
            token_1_mint.key().as_ref()
        ],
        bump,
    )]
    pub token_1_vault: UncheckedAccount<'info>,

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

    /// Program to create mint account and mint tokens
    pub token_program: Program<'info, Token>,
    /// Spl token program or token program 2022
    pub token_0_program: Interface<'info, TokenInterface>,
    /// Spl token program or token program 2022
    pub token_1_program: Interface<'info, TokenInterface>,
    /// Program to create an ATA for receiving position NFT
    pub associated_token_program: Program<'info, AssociatedToken>,
    /// To create a new program account
    pub system_program: Program<'info, System>,
    /// Sysvar for program account
    pub rent: Sysvar<'info, Rent>,
    // remaining account
    // #[account(
    //     seeds = [
    //     SUPPORT_MINT_SEED.as_bytes(),
    //     token_0_mint.key().as_ref(),
    // ],
    //     bump
    // )]
    // pub support_mint0_associated: Account<'info, SupportMintAssociated>,

    // #[account(
    //     seeds = [
    //     SUPPORT_MINT_SEED.as_bytes(),
    //     token_1_mint.key().as_ref(),
    // ],
    //     bump
    // )]
    // pub support_mint1_associated: Account<'info, SupportMintAssociated>,
}

impl<'info> Initialize<'info> {
    pub(crate) fn pool_accounts<'a>(
        &'a self,
        remaining_accounts: &'a [AccountInfo<'info>],
        bumps: &InitializeBumps,
    ) -> InitializePoolAccounts<'a, 'info> {
        InitializePoolAccounts {
            payer: &self.creator,
            pool_creator: self.creator.key(),
            amm_config: &self.amm_config,
            authority: &self.authority,
            pool_state: &self.pool_state,
            token_0_mint: &self.token_0_mint,
            token_1_mint: &self.token_1_mint,
            lp_mint: &self.lp_mint,
            payer_token_0: &self.creator_token_0,
            payer_token_1: &self.creator_token_1,
            payer_lp_token: &self.creator_lp_token,
            token_0_vault: &self.token_0_vault,
            token_1_vault: &self.token_1_vault,
            create_pool_fee: &self.create_pool_fee,
            observation_state: &self.observation_state,
            token_program: &self.token_program,
            token_0_program: &self.token_0_program,
            token_1_program: &self.token_1_program,
            system_program: &self.system_program,
            remaining_accounts,
            authority_bump: bumps.authority,
            token_0_vault_bump: bumps.token_0_vault,
            token_1_vault_bump: bumps.token_1_vault,
        }
    }
}

pub fn initialize<'info>(
    ctx: Context<'info, Initialize<'info>>,
    init_amount_0: u64,
    init_amount_1: u64,
    open_time: u64,
) -> Result<()> {
    initialize_pool(
        ctx.accounts
            .pool_accounts(ctx.remaining_accounts, &ctx.bumps),
        init_amount_0,
        init_amount_1,
        open_time,
        false,
        CreatorFeeOn::BothToken,
    )
}

pub(crate) struct InitializePoolAccounts<'a, 'info> {
    /// Pays for every created account and the create-pool fee, and supplies the initial
    /// liquidity from `payer_token_0` / `payer_token_1`; receives the lp tokens.
    pub payer: &'a Signer<'info>,
    /// Stored as `PoolState::pool_creator`, the receiver of the creator fee.
    pub pool_creator: Pubkey,
    pub amm_config: &'a Account<'info, AmmConfig>,
    pub authority: &'a UncheckedAccount<'info>,
    pub pool_state: &'a UncheckedAccount<'info>,
    pub token_0_mint: &'a InterfaceAccount<'info, Mint>,
    pub token_1_mint: &'a InterfaceAccount<'info, Mint>,
    pub lp_mint: &'a InterfaceAccount<'info, Mint>,
    pub payer_token_0: &'a InterfaceAccount<'info, TokenAccount>,
    pub payer_token_1: &'a InterfaceAccount<'info, TokenAccount>,
    pub payer_lp_token: &'a InterfaceAccount<'info, TokenAccount>,
    pub token_0_vault: &'a UncheckedAccount<'info>,
    pub token_1_vault: &'a UncheckedAccount<'info>,
    pub create_pool_fee: &'a InterfaceAccount<'info, TokenAccount>,
    pub observation_state: &'a AccountLoader<'info, ObservationState>,
    pub token_program: &'a Program<'info, Token>,
    pub token_0_program: &'a Interface<'info, TokenInterface>,
    pub token_1_program: &'a Interface<'info, TokenInterface>,
    pub system_program: &'a Program<'info, System>,
    /// Optional `SupportMintAssociated` accounts for token-2022 mints.
    pub remaining_accounts: &'a [AccountInfo<'info>],
    pub authority_bump: u8,
    pub token_0_vault_bump: u8,
    pub token_1_vault_bump: u8,
}

pub(crate) fn initialize_pool(
    accounts: InitializePoolAccounts,
    init_amount_0: u64,
    init_amount_1: u64,
    mut open_time: u64,
    enable_creator_fee: bool,
    creator_fee_on: CreatorFeeOn,
) -> Result<()> {
    let mint0_associated_is_initialized =
        support_mint_associated_is_initialized(accounts.remaining_accounts, accounts.token_0_mint)?;
    let mint1_associated_is_initialized =
        support_mint_associated_is_initialized(accounts.remaining_accounts, accounts.token_1_mint)?;
    if !(is_supported_mint(accounts.token_0_mint, mint0_associated_is_initialized).unwrap()
        && is_supported_mint(accounts.token_1_mint, mint1_associated_is_initialized).unwrap())
    {
        return err!(ErrorCode::NotSupportMint);
    }

    if accounts.amm_config.disable_create_pool {
        return err!(ErrorCode::NotApproved);
    }
    let block_timestamp = clock::Clock::get()?.unix_timestamp as u64;
    if open_time <= block_timestamp {
        open_time = block_timestamp + 1;
    }
    // due to stack/heap limitations, we have to create redundant new accounts ourselves.
    create_token_account(
        &accounts.authority.to_account_info(),
        &accounts.payer.to_account_info(),
        &accounts.token_0_vault.to_account_info(),
        &accounts.token_0_mint.to_account_info(),
        &accounts.system_program.to_account_info(),
        &accounts.token_0_program.to_account_info(),
        &[
            POOL_VAULT_SEED.as_bytes(),
            accounts.pool_state.key().as_ref(),
            accounts.token_0_mint.key().as_ref(),
            &[accounts.token_0_vault_bump][..],
        ],
    )?;

    create_token_account(
        &accounts.authority.to_account_info(),
        &accounts.payer.to_account_info(),
        &accounts.token_1_vault.to_account_info(),
        &accounts.token_1_mint.to_account_info(),
        &accounts.system_program.to_account_info(),
        &accounts.token_1_program.to_account_info(),
        &[
            POOL_VAULT_SEED.as_bytes(),
            accounts.pool_state.key().as_ref(),
            accounts.token_1_mint.key().as_ref(),
            &[accounts.token_1_vault_bump][..],
        ],
    )?;

    let pool_state_loader = create_pool(
        &accounts.payer.to_account_info(),
        &accounts.pool_state.to_account_info(),
        &accounts.amm_config.to_account_info(),
        &accounts.token_0_mint.to_account_info(),
        &accounts.token_1_mint.to_account_info(),
        &accounts.system_program.to_account_info(),
    )?;
    let pool_state = &mut pool_state_loader.load_init()?;

    let mut observation_state = accounts.observation_state.load_init()?;
    observation_state.pool_id = accounts.pool_state.key();

    transfer_from_user_to_pool_vault(
        accounts.payer.to_account_info(),
        accounts.payer_token_0.to_account_info(),
        accounts.token_0_vault.to_account_info(),
        accounts.token_0_mint.to_account_info(),
        accounts.token_0_program.to_account_info(),
        init_amount_0,
        accounts.token_0_mint.decimals,
    )?;

    transfer_from_user_to_pool_vault(
        accounts.payer.to_account_info(),
        accounts.payer_token_1.to_account_info(),
        accounts.token_1_vault.to_account_info(),
        accounts.token_1_mint.to_account_info(),
        accounts.token_1_program.to_account_info(),
        init_amount_1,
        accounts.token_1_mint.decimals,
    )?;

    let token_0_vault =
        spl_token_2022::extension::StateWithExtensions::<spl_token_2022::state::Account>::unpack(
            accounts
                .token_0_vault
                .to_account_info()
                .try_borrow_data()?
                .deref(),
        )?
        .base;
    let token_1_vault =
        spl_token_2022::extension::StateWithExtensions::<spl_token_2022::state::Account>::unpack(
            accounts
                .token_1_vault
                .to_account_info()
                .try_borrow_data()?
                .deref(),
        )?
        .base;

    CurveCalculator::validate_supply(token_0_vault.amount, token_1_vault.amount)?;

    let liquidity = U128::from(token_0_vault.amount)
        .checked_mul(token_1_vault.amount.into())
        .unwrap()
        .integer_sqrt()
        .as_u64();
    let lock_lp_amount = 100;
    msg!(
        "liquidity:{}, lock_lp_amount:{}, vault_0_amount:{},vault_1_amount:{}",
        liquidity,
        lock_lp_amount,
        token_0_vault.amount,
        token_1_vault.amount
    );
    token::token_mint_to(
        accounts.authority.to_account_info(),
        accounts.token_program.to_account_info(),
        accounts.lp_mint.to_account_info(),
        accounts.payer_lp_token.to_account_info(),
        liquidity
            .checked_sub(lock_lp_amount)
            .ok_or(ErrorCode::InitLpAmountTooLess)?,
        &[&[crate::AUTH_SEED.as_bytes(), &[accounts.authority_bump]]],
    )?;

    // Charge the fee to create a pool
    if accounts.amm_config.create_pool_fee != 0 {
        invoke(
            &system_instruction::transfer(
                accounts.payer.key,
                &accounts.create_pool_fee.key(),
                u64::from(accounts.amm_config.create_pool_fee),
            ),
            &[
                accounts.payer.to_account_info(),
                accounts.create_pool_fee.to_account_info(),
                accounts.system_program.to_account_info(),
            ],
        )?;
        invoke(
            &spl_token::instruction::sync_native(
                accounts.token_program.key,
                &accounts.create_pool_fee.key(),
            )?,
            &[
                accounts.token_program.to_account_info(),
                accounts.create_pool_fee.to_account_info(),
            ],
        )?;
    }

    pool_state.initialize(
        accounts.authority_bump,
        liquidity,
        open_time,
        accounts.pool_creator,
        accounts.amm_config.key(),
        accounts.token_0_vault.key(),
        accounts.token_1_vault.key(),
        accounts.token_0_mint,
        accounts.token_1_mint,
        accounts.lp_mint.key(),
        accounts.lp_mint.decimals,
        accounts.observation_state.key(),
        creator_fee_on,
        enable_creator_fee,
    );

    Ok(())
}

pub fn create_pool<'info>(
    payer: &AccountInfo<'info>,
    pool_account_info: &AccountInfo<'info>,
    amm_config: &AccountInfo<'info>,
    token_0_mint: &AccountInfo<'info>,
    token_1_mint: &AccountInfo<'info>,
    system_program: &AccountInfo<'info>,
) -> Result<AccountLoad<'info, PoolState>> {
    if pool_account_info.owner != &system_program::ID {
        return err!(ErrorCode::NotApproved);
    }

    let (expect_pda_address, bump) = Pubkey::find_program_address(
        &[
            POOL_SEED.as_bytes(),
            amm_config.key().as_ref(),
            token_0_mint.key().as_ref(),
            token_1_mint.key().as_ref(),
        ],
        &crate::id(),
    );

    if pool_account_info.key() != expect_pda_address {
        require_eq!(pool_account_info.is_signer, true);
    }

    token::create_or_allocate_account(
        &crate::id(),
        payer.to_account_info(),
        system_program.to_account_info(),
        pool_account_info.clone(),
        &[
            POOL_SEED.as_bytes(),
            amm_config.key().as_ref(),
            token_0_mint.key().as_ref(),
            token_1_mint.key().as_ref(),
            &[bump],
        ],
        PoolState::LEN,
    )?;

    Ok(AccountLoad::<PoolState>::try_from_unchecked(
        &crate::id(),
        &pool_account_info,
    )?)
}
