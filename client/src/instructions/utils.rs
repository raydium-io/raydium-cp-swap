use anchor_lang::AccountDeserialize;
use anchor_spl::token_2022::spl_token_2022::{
    extension::{
        transfer_fee::{TransferFeeConfig, MAX_FEE_BASIS_POINTS},
        BaseState, BaseStateWithExtensions, PodStateWithExtensions,
    },
    pod::{PodAccount, PodMint},
};
use anyhow::Result;
use bytemuck::Pod;
use solana_client::rpc_client::RpcClient;
use solana_sdk::{account::Account as CliAccount, pubkey::Pubkey};
use std::ops::Mul;

pub fn deserialize_anchor_account<T: AccountDeserialize>(account: &CliAccount) -> Result<T> {
    let mut data: &[u8] = &account.data;
    T::try_deserialize(&mut data).map_err(Into::into)
}

pub fn unpack_token(token_data: &[u8]) -> Result<PodStateWithExtensions<PodAccount>> {
    let token = PodStateWithExtensions::<PodAccount>::unpack(&token_data)?;
    Ok(token)
}

pub fn unpack_mint(token_data: &[u8]) -> Result<PodStateWithExtensions<PodMint>> {
    let mint = PodStateWithExtensions::<PodMint>::unpack(&token_data)?;
    Ok(mint)
}

#[derive(Debug)]
pub struct TransferFeeInfo {
    pub mint: Pubkey,
    pub owner: Pubkey,
    pub transfer_fee: u64,
}

pub fn amount_with_slippage(amount: u64, slippage: f64, round_up: bool) -> u64 {
    if round_up {
        (amount as f64).mul(1_f64 + slippage).ceil() as u64
    } else {
        (amount as f64).mul(1_f64 - slippage).floor() as u64
    }
}

pub fn get_pool_mints_inverse_fee(
    rpc_client: &RpcClient,
    token_mint_0: Pubkey,
    token_mint_1: Pubkey,
    post_fee_amount_0: u64,
    post_fee_amount_1: u64,
) -> (TransferFeeInfo, TransferFeeInfo) {
    let load_accounts = vec![token_mint_0, token_mint_1];
    let rsps = rpc_client.get_multiple_accounts(&load_accounts).unwrap();
    let epoch = rpc_client.get_epoch_info().unwrap().epoch;
    let mint0_account = rsps[0].clone().ok_or("load mint0 rps error!").unwrap();
    let mint1_account = rsps[1].clone().ok_or("load mint0 rps error!").unwrap();
    let mint0_state = unpack_mint(&mint0_account.data).unwrap();
    let mint1_state = unpack_mint(&mint1_account.data).unwrap();
    (
        TransferFeeInfo {
            mint: token_mint_0,
            owner: mint0_account.owner,
            transfer_fee: get_transfer_inverse_fee(&mint0_state, post_fee_amount_0, epoch),
        },
        TransferFeeInfo {
            mint: token_mint_1,
            owner: mint1_account.owner,
            transfer_fee: get_transfer_inverse_fee(&mint1_state, post_fee_amount_1, epoch),
        },
    )
}

pub fn get_pool_mints_transfer_fee(
    rpc_client: &RpcClient,
    token_mint_0: Pubkey,
    token_mint_1: Pubkey,
    pre_fee_amount_0: u64,
    pre_fee_amount_1: u64,
) -> (TransferFeeInfo, TransferFeeInfo) {
    let load_accounts = vec![token_mint_0, token_mint_1];
    let rsps = rpc_client.get_multiple_accounts(&load_accounts).unwrap();
    let epoch = rpc_client.get_epoch_info().unwrap().epoch;
    let mint0_account = rsps[0].clone().ok_or("load mint0 rps error!").unwrap();
    let mint1_account = rsps[1].clone().ok_or("load mint0 rps error!").unwrap();
    let mint0_state = unpack_mint(&mint0_account.data).unwrap();
    let mint1_state = unpack_mint(&mint1_account.data).unwrap();
    (
        TransferFeeInfo {
            mint: token_mint_0,
            owner: mint0_account.owner,
            transfer_fee: get_transfer_fee(&mint0_state, pre_fee_amount_0, epoch),
        },
        TransferFeeInfo {
            mint: token_mint_1,
            owner: mint1_account.owner,
            transfer_fee: get_transfer_fee(&mint1_state, pre_fee_amount_1, epoch),
        },
    )
}

/// Calculate the fee for output amount
pub fn get_transfer_inverse_fee<'data, S: BaseState + Pod>(
    account_state: &PodStateWithExtensions<'data, S>,
    epoch: u64,
    post_fee_amount: u64,
) -> u64 {
    let fee = if let Ok(transfer_fee_config) = account_state.get_extension::<TransferFeeConfig>() {
        let transfer_fee = transfer_fee_config.get_epoch_fee(epoch);
        if u16::from(transfer_fee.transfer_fee_basis_points) == MAX_FEE_BASIS_POINTS {
            u64::from(transfer_fee.maximum_fee)
        } else {
            transfer_fee_config
                .calculate_inverse_epoch_fee(epoch, post_fee_amount)
                .unwrap()
        }
    } else {
        0
    };
    fee
}

/// Calculate the fee for input amount
pub fn get_transfer_fee<'data, S: BaseState + Pod>(
    account_state: &PodStateWithExtensions<'data, S>,
    epoch: u64,
    pre_fee_amount: u64,
) -> u64 {
    let fee = if let Ok(transfer_fee_config) = account_state.get_extension::<TransferFeeConfig>() {
        transfer_fee_config
            .calculate_epoch_fee(epoch, pre_fee_amount)
            .unwrap()
    } else {
        0
    };
    fee
}
