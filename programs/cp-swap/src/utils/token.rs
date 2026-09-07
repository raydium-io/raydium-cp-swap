use crate::error::ErrorCode;
use crate::states::*;
use anchor_lang::{prelude::*, system_program};
use anchor_spl::token_2022::spl_token_2022::{
    self,
    extension::{
        transfer_fee::{TransferFeeConfig, MAX_FEE_BASIS_POINTS},
        BaseStateWithExtensions, ExtensionType, StateWithExtensions,
    },
};
use anchor_spl::{
    token::{Token, TokenAccount},
    token_2022::{self},
    token_interface::{initialize_account3, InitializeAccount3, Mint},
};

pub fn transfer_from_user_to_pool_vault<'a>(
    authority: AccountInfo<'a>,
    from: AccountInfo<'a>,
    to_vault: AccountInfo<'a>,
    mint: AccountInfo<'a>,
    token_program: AccountInfo<'a>,
    amount: u64,
    mint_decimals: u8,
) -> Result<()> {
    if amount == 0 {
        return Ok(());
    }
    token_2022::transfer_checked(
        CpiContext::new(
            *token_program.key,
            token_2022::TransferChecked {
                from,
                to: to_vault,
                authority,
                mint,
            },
        ),
        amount,
        mint_decimals,
    )
}

pub fn transfer_from_pool_vault_to_user<'a>(
    authority: AccountInfo<'a>,
    from_vault: AccountInfo<'a>,
    to: AccountInfo<'a>,
    mint: AccountInfo<'a>,
    token_program: AccountInfo<'a>,
    amount: u64,
    mint_decimals: u8,
    signer_seeds: &[&[&[u8]]],
) -> Result<()> {
    if amount == 0 {
        return Ok(());
    }
    token_2022::transfer_checked(
        CpiContext::new_with_signer(
            *token_program.key,
            token_2022::TransferChecked {
                from: from_vault,
                to,
                authority,
                mint,
            },
            signer_seeds,
        ),
        amount,
        mint_decimals,
    )
}

/// Issue a spl_token `MintTo` instruction.
pub fn token_mint_to<'a>(
    authority: AccountInfo<'a>,
    token_program: AccountInfo<'a>,
    mint: AccountInfo<'a>,
    destination: AccountInfo<'a>,
    amount: u64,
    signer_seeds: &[&[&[u8]]],
) -> Result<()> {
    token_2022::mint_to(
        CpiContext::new_with_signer(
            *token_program.key,
            token_2022::MintTo {
                to: destination,
                authority,
                mint,
            },
            signer_seeds,
        ),
        amount,
    )
}

pub fn token_burn<'a>(
    authority: AccountInfo<'a>,
    token_program: AccountInfo<'a>,
    mint: AccountInfo<'a>,
    from: AccountInfo<'a>,
    amount: u64,
    signer_seeds: &[&[&[u8]]],
) -> Result<()> {
    token_2022::burn(
        CpiContext::new_with_signer(
            *token_program.key,
            token_2022::Burn {
                from,
                authority,
                mint,
            },
            signer_seeds,
        ),
        amount,
    )
}

pub fn withdraw_excess_lamports<'a>(
    token_program: AccountInfo<'a>,
    source: AccountInfo<'a>,
    destination: AccountInfo<'a>,
    authority: AccountInfo<'a>,
    amm_seed: &[u8],
    nonce: u8,
) -> Result<()> {
    let ix = instruction::Instruction {
        program_id: *token_program.key,
        accounts: vec![
            AccountMeta::new(*source.key, false),
            AccountMeta::new(*destination.key, false),
            AccountMeta::new_readonly(*authority.key, true),
        ],
        data: vec![38], // TokenInstruction::WithdrawExcessLamports = 38
    };
    anchor_lang::solana_program::program::invoke_signed(
        &ix,
        &[source, destination, authority, token_program],
        &[&[amm_seed, &[nonce]]],
    )
    .map_err(Into::into)
}

pub fn unwrap_lamports<'a>(
    token_program: AccountInfo<'a>,
    source: AccountInfo<'a>,
    destination: AccountInfo<'a>,
    authority: AccountInfo<'a>,
    amm_seed: &[u8],
    nonce: u8,
    amount: Option<u64>,
) -> Result<()> {
    // TokenInstruction::UnwrapLamports = 45, followed by a COption<u64>
    let mut data = vec![45];
    match amount {
        Some(amount) => {
            data.push(1); // COption::Some
            data.extend_from_slice(&amount.to_le_bytes());
        }
        None => data.push(0), // COption::None
    }
    let ix = instruction::Instruction {
        program_id: *token_program.key,
        accounts: vec![
            AccountMeta::new(*source.key, false),
            AccountMeta::new(*destination.key, false),
            AccountMeta::new_readonly(*authority.key, true),
        ],
        data,
    };
    anchor_lang::solana_program::program::invoke_signed(
        &ix,
        &[source, destination, authority, token_program],
        &[&[amm_seed, &[nonce]]],
    )
    .map_err(Into::into)
}

/// Read (is_native, amount) from a token account, parsing extensions so it
/// works for both legacy accounts and Token-2022 accounts that carry extensions.
fn token_account_native_and_amount(account: &AccountInfo) -> Result<(bool, u64)> {
    let data = account.try_borrow_data()?;
    if let Ok(state) = StateWithExtensions::<spl_token_2022::state::Account>::unpack(&data) {
        return Ok((state.base.is_native.is_some(), state.base.amount));
    } else {
        // process token mint account
        return Ok((false, 0));
    }
}

/// Collect the excess lamports sitting on a token account owned by `authority`.
///
/// A native (WSOL) account cannot use `WithdrawExcessLamports` (the token
/// program rejects native accounts). Instead `SyncNative` folds the donated
/// excess lamports into the wrapped `amount`, the delta is measured, and
/// `UnwrapLamports` pulls exactly that delta back out — leaving the wrapped
/// balance unchanged, which is asserted afterwards.
pub fn withdraw_excess_lamports_from_token<'a>(
    token_program: AccountInfo<'a>,
    source: AccountInfo<'a>,
    destination: AccountInfo<'a>,
    authority: AccountInfo<'a>,
    amm_seed: &[u8],
    nonce: u8,
) -> Result<()> {
    let (is_native, amount_before_sync) = token_account_native_and_amount(&source)?;

    if !is_native {
        return withdraw_excess_lamports(
            token_program,
            source,
            destination,
            authority,
            amm_seed,
            nonce,
        );
    }

    // SyncNative (ix 17) folds the donated excess lamports into the wrapped amount.
    let sync_ix = spl_token_2022::instruction::sync_native(token_program.key, source.key)?;
    anchor_lang::solana_program::program::invoke(
        &sync_ix,
        &[source.clone(), token_program.clone()],
    )?;

    let (_, amount_after_sync) = token_account_native_and_amount(&source)?;
    let excess_lamports = amount_after_sync
        .checked_sub(amount_before_sync)
        .ok_or(ErrorCode::LamportsCalculateError)?;
    if excess_lamports == 0 {
        return Ok(());
    }

    unwrap_lamports(
        token_program,
        source.clone(),
        destination,
        authority,
        amm_seed,
        nonce,
        Some(excess_lamports),
    )?;

    // The wrapped balance must be exactly what it was before sync + unwrap.
    let (_, amount_after_unwrap) = token_account_native_and_amount(&source)?;
    require_eq!(
        amount_before_sync,
        amount_after_unwrap,
        ErrorCode::LamportsCalculateError
    );
    Ok(())
}

/// Calculate the fee for output amount
pub fn get_transfer_inverse_fee(mint_info: &AccountInfo, post_fee_amount: u64) -> Result<u64> {
    if *mint_info.owner == Token::id() {
        return Ok(0);
    }
    if post_fee_amount == 0 {
        return err!(ErrorCode::InvalidInput);
    }
    let mint_data = mint_info.try_borrow_data()?;
    let mint = StateWithExtensions::<spl_token_2022::state::Mint>::unpack(&mint_data)?;

    let fee = if let Ok(transfer_fee_config) = mint.get_extension::<TransferFeeConfig>() {
        let epoch = Clock::get()?.epoch;

        let transfer_fee = transfer_fee_config.get_epoch_fee(epoch);
        if u16::from(transfer_fee.transfer_fee_basis_points) == MAX_FEE_BASIS_POINTS {
            u64::from(transfer_fee.maximum_fee)
        } else {
            let transfer_fee = transfer_fee_config
                .calculate_inverse_epoch_fee(epoch, post_fee_amount)
                .unwrap();
            let transfer_fee_for_check = transfer_fee_config
                .calculate_epoch_fee(epoch, post_fee_amount.checked_add(transfer_fee).unwrap())
                .unwrap();
            if transfer_fee != transfer_fee_for_check {
                return err!(ErrorCode::TransferFeeCalculateNotMatch);
            }
            transfer_fee
        }
    } else {
        0
    };
    Ok(fee)
}

/// Calculate the fee for input amount
pub fn get_transfer_fee(mint_info: &AccountInfo, pre_fee_amount: u64) -> Result<u64> {
    if *mint_info.owner == Token::id() {
        return Ok(0);
    }
    let mint_data = mint_info.try_borrow_data()?;
    let mint = StateWithExtensions::<spl_token_2022::state::Mint>::unpack(&mint_data)?;

    let fee = if let Ok(transfer_fee_config) = mint.get_extension::<TransferFeeConfig>() {
        transfer_fee_config
            .calculate_epoch_fee(Clock::get()?.epoch, pre_fee_amount)
            .unwrap()
    } else {
        0
    };
    Ok(fee)
}

pub fn support_mint_associated_is_initialized(
    remaining_accounts: &[AccountInfo],
    token_mint: &InterfaceAccount<Mint>,
) -> Result<bool> {
    if remaining_accounts.len() == 0 {
        return Ok(false);
    }
    let (expect_mint_associated, __bump) = Pubkey::find_program_address(
        &[SUPPORT_MINT_SEED.as_bytes(), token_mint.key().as_ref()],
        &crate::id(),
    );
    let mut mint_associated_is_initialized = false;
    for mint_associated_info in remaining_accounts.into_iter() {
        if *mint_associated_info.owner != crate::id()
            || mint_associated_info.key() != expect_mint_associated
        {
            continue;
        }
        let mint_associated = SupportMintAssociated::try_deserialize(
            &mut mint_associated_info.data.borrow().as_ref(),
        )?;
        if mint_associated.mint == token_mint.key() {
            mint_associated_is_initialized = true;
            break;
        }
    }
    return Ok(mint_associated_is_initialized);
}

pub fn is_supported_mint(
    mint_account: &InterfaceAccount<Mint>,
    mint_associated_is_initialized: bool,
) -> Result<bool> {
    let mint_info = mint_account.to_account_info();
    if *mint_info.owner == Token::id() {
        return Ok(true);
    }
    if mint_associated_is_initialized {
        return Ok(true);
    }
    let mint_data = mint_info.try_borrow_data()?;
    let mint = StateWithExtensions::<spl_token_2022::state::Mint>::unpack(&mint_data)?;
    let extensions = mint.get_extension_types()?;
    for e in extensions {
        if e != ExtensionType::TransferFeeConfig
            && e != ExtensionType::MetadataPointer
            && e != ExtensionType::TokenMetadata
            && e != ExtensionType::InterestBearingConfig
            && e != ExtensionType::ScaledUiAmount
        {
            return Ok(false);
        }
    }
    Ok(true)
}

pub fn create_token_account<'a>(
    authority: &AccountInfo<'a>,
    payer: &AccountInfo<'a>,
    token_account: &AccountInfo<'a>,
    mint_account: &AccountInfo<'a>,
    system_program: &AccountInfo<'a>,
    token_program: &AccountInfo<'a>,
    signer_seeds: &[&[u8]],
) -> Result<()> {
    let space = {
        let mint_info = mint_account.to_account_info();
        if *mint_info.owner == token_2022::Token2022::id() {
            let mint_data = mint_info.try_borrow_data()?;
            let mint_state =
                StateWithExtensions::<spl_token_2022::state::Mint>::unpack(&mint_data)?;
            let mint_extensions = mint_state.get_extension_types()?;
            let required_extensions =
                ExtensionType::get_required_init_account_extensions(&mint_extensions);
            ExtensionType::try_calculate_account_len::<spl_token_2022::state::Account>(
                &required_extensions,
            )?
        } else {
            TokenAccount::LEN
        }
    };
    create_or_allocate_account(
        token_program.key,
        payer.to_account_info(),
        system_program.to_account_info(),
        token_account.to_account_info(),
        signer_seeds,
        space,
    )?;
    initialize_account3(CpiContext::new(
        *token_program.key,
        InitializeAccount3 {
            account: token_account.to_account_info(),
            mint: mint_account.to_account_info(),
            authority: authority.to_account_info(),
        },
    ))
}

pub fn create_or_allocate_account<'a>(
    program_id: &Pubkey,
    payer: AccountInfo<'a>,
    system_program: AccountInfo<'a>,
    target_account: AccountInfo<'a>,
    siger_seed: &[&[u8]],
    space: usize,
) -> Result<()> {
    let rent = Rent::get()?;
    let current_lamports = target_account.lamports();

    if current_lamports == 0 {
        let lamports = rent.minimum_balance(space);
        let cpi_accounts = system_program::CreateAccount {
            from: payer,
            to: target_account.clone(),
        };
        let cpi_context = CpiContext::new(*system_program.key, cpi_accounts);
        system_program::create_account(
            cpi_context.with_signer(&[siger_seed]),
            lamports,
            u64::try_from(space).unwrap(),
            program_id,
        )?;
    } else {
        let required_lamports = rent
            .minimum_balance(space)
            .max(1)
            .saturating_sub(current_lamports);
        if required_lamports > 0 {
            let cpi_accounts = system_program::Transfer {
                from: payer.to_account_info(),
                to: target_account.clone(),
            };
            let cpi_context = CpiContext::new(*system_program.key, cpi_accounts);
            system_program::transfer(cpi_context, required_lamports)?;
        }
        let cpi_accounts = system_program::Allocate {
            account_to_allocate: target_account.clone(),
        };
        let cpi_context = CpiContext::new(*system_program.key, cpi_accounts);
        system_program::allocate(
            cpi_context.with_signer(&[siger_seed]),
            u64::try_from(space).unwrap(),
        )?;

        let cpi_accounts = system_program::Assign {
            account_to_assign: target_account.clone(),
        };
        let cpi_context = CpiContext::new(*system_program.key, cpi_accounts);
        system_program::assign(cpi_context.with_signer(&[siger_seed]), program_id)?;
    }
    Ok(())
}
