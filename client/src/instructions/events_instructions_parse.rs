use anchor_client::ClientError;
use anchor_lang::Discriminator;
use anyhow::Result;
use colorful::Color;
use colorful::Colorful;
use raydium_cp_swap::instruction;
use raydium_cp_swap::states::*;
use regex::Regex;
use solana_transaction_status::{
    option_serializer::OptionSerializer, EncodedTransaction, UiTransactionStatusMeta,
};

const PROGRAM_LOG: &str = "Program log: ";
const PROGRAM_DATA: &str = "Program data: ";

pub enum InstructionDecodeType {
    BaseHex,
    Base64,
    Base58,
}

pub fn parse_program_event(
    self_program_str: &str,
    meta: Option<UiTransactionStatusMeta>,
) -> Result<(), ClientError> {
    let logs: Vec<String> = if let Some(meta_data) = meta {
        let log_messages = if let OptionSerializer::Some(log_messages) = meta_data.log_messages {
            log_messages
        } else {
            Vec::new()
        };
        log_messages
    } else {
        Vec::new()
    };
    let mut logs = &logs[..];
    if !logs.is_empty() {
        if let Ok(mut execution) = Execution::new(&mut logs) {
            for l in logs {
                let (new_program, did_pop) =
                    if !execution.is_empty() && self_program_str == execution.program() {
                        handle_program_log(self_program_str, &l, true).unwrap_or_else(|e| {
                            println!("Unable to parse log: {e}");
                            std::process::exit(1);
                        })
                    } else {
                        let (program, did_pop) = handle_system_log(self_program_str, l);
                        (program, did_pop)
                    };
                // Switch program context on CPI.
                if let Some(new_program) = new_program {
                    execution.push(new_program);
                }
                // Program returned.
                if did_pop {
                    execution.pop();
                }
            }
        }
    } else {
        println!("log is empty");
    }
    Ok(())
}

struct Execution {
    stack: Vec<String>,
}

impl Execution {
    pub fn new(logs: &mut &[String]) -> Result<Self, ClientError> {
        let l = &logs[0];
        *logs = &logs[1..];

        let re = Regex::new(r"^Program (.*) invoke.*$").unwrap();
        let c = re
            .captures(l)
            .ok_or_else(|| ClientError::LogParseError(l.to_string()))?;
        let program = c
            .get(1)
            .ok_or_else(|| ClientError::LogParseError(l.to_string()))?
            .as_str()
            .to_string();
        Ok(Self {
            stack: vec![program],
        })
    }

    pub fn program(&self) -> String {
        assert!(!self.stack.is_empty());
        self.stack[self.stack.len() - 1].clone()
    }

    pub fn is_empty(&self) -> bool {
        self.stack.is_empty()
    }

    pub fn push(&mut self, new_program: String) {
        self.stack.push(new_program);
    }

    pub fn pop(&mut self) {
        assert!(!self.stack.is_empty());
        self.stack.pop().unwrap();
    }
}

pub fn handle_program_log(
    self_program_str: &str,
    l: &str,
    with_prefix: bool,
) -> Result<(Option<String>, bool), ClientError> {
    // Log emitted from the current program.
    if let Some(log) = if with_prefix {
        l.strip_prefix(PROGRAM_LOG)
            .or_else(|| l.strip_prefix(PROGRAM_DATA))
    } else {
        Some(l)
    } {
        if l.starts_with(&format!("Program log:")) {
            // not log event
            return Ok((None, false));
        }
        let borsh_bytes = match anchor_lang::__private::base64::decode(log) {
            Ok(borsh_bytes) => borsh_bytes,
            _ => {
                println!("Could not base64 decode log: {}", log);
                return Ok((None, false));
            }
        };

        let mut slice: &[u8] = &borsh_bytes[..];
        let disc: [u8; 8] = {
            let mut disc = [0; 8];
            disc.copy_from_slice(&borsh_bytes[..8]);
            slice = &slice[8..];
            disc
        };
        match disc {
            SwapEvent::DISCRIMINATOR => {
                println!("{:#?}", decode_event::<SwapEvent>(&mut slice)?);
            }
            LpChangeEvent::DISCRIMINATOR => {
                println!("{:#?}", decode_event::<LpChangeEvent>(&mut slice)?);
            }
            _ => {
                println!("unknow event: {}", l);
            }
        }
        return Ok((None, false));
    } else {
        let (program, did_pop) = handle_system_log(self_program_str, l);
        return Ok((program, did_pop));
    }
}

fn handle_system_log(this_program_str: &str, log: &str) -> (Option<String>, bool) {
    if log.starts_with(&format!("Program {this_program_str} invoke")) {
        (Some(this_program_str.to_string()), false)
    } else if log.contains("invoke") {
        (Some("cpi".to_string()), false) // Any string will do.
    } else {
        let re = Regex::new(r"^Program (.*) success*$").unwrap();
        if re.is_match(log) {
            (None, true)
        } else {
            (None, false)
        }
    }
}

fn decode_event<T: anchor_lang::Event + anchor_lang::AnchorDeserialize>(
    slice: &mut &[u8],
) -> Result<T, ClientError> {
    let event: T = anchor_lang::AnchorDeserialize::deserialize(slice)
        .map_err(|e| ClientError::LogParseError(e.to_string()))?;
    Ok(event)
}

pub fn parse_program_instruction(
    self_program_str: &str,
    encoded_transaction: EncodedTransaction,
    meta: Option<UiTransactionStatusMeta>,
) -> Result<(), ClientError> {
    let ui_raw_msg = match encoded_transaction {
        solana_transaction_status::EncodedTransaction::Json(ui_tx) => {
            let ui_message = ui_tx.message;
            // println!("{:#?}", ui_message);
            match ui_message {
                solana_transaction_status::UiMessage::Raw(ui_raw_msg) => ui_raw_msg,
                _ => solana_transaction_status::UiRawMessage {
                    header: solana_sdk::message::MessageHeader::default(),
                    account_keys: Vec::new(),
                    recent_blockhash: "".to_string(),
                    instructions: Vec::new(),
                    address_table_lookups: None,
                },
            }
        }
        _ => solana_transaction_status::UiRawMessage {
            header: solana_sdk::message::MessageHeader::default(),
            account_keys: Vec::new(),
            recent_blockhash: "".to_string(),
            instructions: Vec::new(),
            address_table_lookups: None,
        },
    };
    // append lookup table keys if necessary
    if meta.is_some() {
        let mut account_keys = ui_raw_msg.account_keys;
        let meta = meta.clone().unwrap();
        match meta.loaded_addresses {
            OptionSerializer::Some(addresses) => {
                let mut writeable_address = addresses.writable;
                let mut readonly_address = addresses.readonly;
                account_keys.append(&mut writeable_address);
                account_keys.append(&mut readonly_address);
            }
            _ => {}
        }
        let program_index = account_keys
            .iter()
            .position(|r| r == self_program_str)
            .unwrap();
        // println!("{}", program_index);
        // println!("{:#?}", account_keys);
        for (i, ui_compiled_instruction) in ui_raw_msg.instructions.iter().enumerate() {
            if (ui_compiled_instruction.program_id_index as usize) == program_index {
                let out_put = format!("instruction #{}", i + 1);
                println!("{}", out_put.gradient(Color::Green));
                handle_program_instruction(
                    &ui_compiled_instruction.data,
                    InstructionDecodeType::Base58,
                )?;
            }
        }

        match meta.inner_instructions {
            OptionSerializer::Some(inner_instructions) => {
                for inner in inner_instructions {
                    for (i, instruction) in inner.instructions.iter().enumerate() {
                        match instruction {
                            solana_transaction_status::UiInstruction::Compiled(
                                ui_compiled_instruction,
                            ) => {
                                if (ui_compiled_instruction.program_id_index as usize)
                                    == program_index
                                {
                                    let out_put =
                                        format!("inner_instruction #{}.{}", inner.index + 1, i + 1);
                                    println!("{}", out_put.gradient(Color::Green));
                                    handle_program_instruction(
                                        &ui_compiled_instruction.data,
                                        InstructionDecodeType::Base58,
                                    )?;
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
            _ => {}
        }
    }
    Ok(())
}

pub fn handle_program_instruction(
    instr_data: &str,
    decode_type: InstructionDecodeType,
) -> Result<(), ClientError> {
    let data;
    match decode_type {
        InstructionDecodeType::BaseHex => {
            data = hex::decode(instr_data).unwrap();
        }
        InstructionDecodeType::Base64 => {
            let borsh_bytes = match anchor_lang::__private::base64::decode(instr_data) {
                Ok(borsh_bytes) => borsh_bytes,
                _ => {
                    println!("Could not base64 decode instruction: {}", instr_data);
                    return Ok(());
                }
            };
            data = borsh_bytes;
        }
        InstructionDecodeType::Base58 => {
            let borsh_bytes = match bs58::decode(instr_data).into_vec() {
                Ok(borsh_bytes) => borsh_bytes,
                _ => {
                    println!("Could not base58 decode instruction: {}", instr_data);
                    return Ok(());
                }
            };
            data = borsh_bytes;
        }
    }

    let mut ix_data: &[u8] = &data[..];
    let disc: [u8; 8] = {
        let mut disc = [0; 8];
        disc.copy_from_slice(&data[..8]);
        ix_data = &ix_data[8..];
        disc
    };
    // println!("{:?}", disc);

    match disc {
        instruction::CreateAmmConfig::DISCRIMINATOR => {
            let ix = decode_instruction::<instruction::CreateAmmConfig>(&mut ix_data).unwrap();
            #[derive(Debug)]
            pub struct CreateAmmConfig {
                pub index: u16,
                pub trade_fee_rate: u64,
                pub protocol_fee_rate: u64,
                pub fund_fee_rate: u64,
                pub create_pool_fee: u64,
            }
            impl From<instruction::CreateAmmConfig> for CreateAmmConfig {
                fn from(instr: instruction::CreateAmmConfig) -> CreateAmmConfig {
                    CreateAmmConfig {
                        index: instr.index,
                        trade_fee_rate: instr.trade_fee_rate,
                        protocol_fee_rate: instr.protocol_fee_rate,
                        fund_fee_rate: instr.fund_fee_rate,
                        create_pool_fee: instr.create_pool_fee,
                    }
                }
            }
            println!("{:#?}", CreateAmmConfig::from(ix));
        }
        instruction::UpdateAmmConfig::DISCRIMINATOR => {
            let ix = decode_instruction::<instruction::UpdateAmmConfig>(&mut ix_data).unwrap();
            #[derive(Debug)]
            pub struct UpdateAmmConfig {
                pub param: u8,
                pub value: u64,
            }
            impl From<instruction::UpdateAmmConfig> for UpdateAmmConfig {
                fn from(instr: instruction::UpdateAmmConfig) -> UpdateAmmConfig {
                    UpdateAmmConfig {
                        param: instr.param,
                        value: instr.value,
                    }
                }
            }
            println!("{:#?}", UpdateAmmConfig::from(ix));
        }
        instruction::Initialize::DISCRIMINATOR => {
            let ix = decode_instruction::<instruction::Initialize>(&mut ix_data).unwrap();
            #[derive(Debug)]
            pub struct Initialize {
                pub init_amount_0: u64,
                pub init_amount_1: u64,
                pub open_time: u64,
            }
            impl From<instruction::Initialize> for Initialize {
                fn from(instr: instruction::Initialize) -> Initialize {
                    Initialize {
                        init_amount_0: instr.init_amount_0,
                        init_amount_1: instr.init_amount_1,
                        open_time: instr.open_time,
                    }
                }
            }
            println!("{:#?}", Initialize::from(ix));
        }
        instruction::UpdatePoolStatus::DISCRIMINATOR => {
            let ix = decode_instruction::<instruction::UpdatePoolStatus>(&mut ix_data).unwrap();
            #[derive(Debug)]
            pub struct UpdatePoolStatus {
                pub status: u8,
            }
            impl From<instruction::UpdatePoolStatus> for UpdatePoolStatus {
                fn from(instr: instruction::UpdatePoolStatus) -> UpdatePoolStatus {
                    UpdatePoolStatus {
                        status: instr.status,
                    }
                }
            }
            println!("{:#?}", UpdatePoolStatus::from(ix));
        }
        instruction::CollectProtocolFee::DISCRIMINATOR => {
            let ix = decode_instruction::<instruction::CollectProtocolFee>(&mut ix_data).unwrap();
            #[derive(Debug)]
            pub struct CollectProtocolFee {
                pub amount_0_requested: u64,
                pub amount_1_requested: u64,
            }
            impl From<instruction::CollectProtocolFee> for CollectProtocolFee {
                fn from(instr: instruction::CollectProtocolFee) -> CollectProtocolFee {
                    CollectProtocolFee {
                        amount_0_requested: instr.amount_0_requested,
                        amount_1_requested: instr.amount_1_requested,
                    }
                }
            }
            println!("{:#?}", CollectProtocolFee::from(ix));
        }
        instruction::CollectFundFee::DISCRIMINATOR => {
            let ix = decode_instruction::<instruction::CollectFundFee>(&mut ix_data).unwrap();
            #[derive(Debug)]
            pub struct CollectFundFee {
                pub amount_0_requested: u64,
                pub amount_1_requested: u64,
            }
            impl From<instruction::CollectFundFee> for CollectFundFee {
                fn from(instr: instruction::CollectFundFee) -> CollectFundFee {
                    CollectFundFee {
                        amount_0_requested: instr.amount_0_requested,
                        amount_1_requested: instr.amount_1_requested,
                    }
                }
            }
            println!("{:#?}", CollectFundFee::from(ix));
        }
        instruction::Deposit::DISCRIMINATOR => {
            let ix = decode_instruction::<instruction::Deposit>(&mut ix_data).unwrap();
            #[derive(Debug)]
            pub struct Deposit {
                pub lp_token_amount: u64,
                pub maximum_token_0_amount: u64,
                pub maximum_token_1_amount: u64,
            }
            impl From<instruction::Deposit> for Deposit {
                fn from(instr: instruction::Deposit) -> Deposit {
                    Deposit {
                        lp_token_amount: instr.lp_token_amount,
                        maximum_token_0_amount: instr.maximum_token_0_amount,
                        maximum_token_1_amount: instr.maximum_token_1_amount,
                    }
                }
            }
            println!("{:#?}", Deposit::from(ix));
        }
        instruction::Withdraw::DISCRIMINATOR => {
            let ix = decode_instruction::<instruction::Withdraw>(&mut ix_data).unwrap();
            #[derive(Debug)]
            pub struct Withdraw {
                pub lp_token_amount: u64,
                pub minimum_token_0_amount: u64,
                pub minimum_token_1_amount: u64,
            }
            impl From<instruction::Withdraw> for Withdraw {
                fn from(instr: instruction::Withdraw) -> Withdraw {
                    Withdraw {
                        lp_token_amount: instr.lp_token_amount,
                        minimum_token_0_amount: instr.minimum_token_0_amount,
                        minimum_token_1_amount: instr.minimum_token_1_amount,
                    }
                }
            }
            println!("{:#?}", Withdraw::from(ix));
        }
        instruction::SwapBaseInput::DISCRIMINATOR => {
            let ix = decode_instruction::<instruction::SwapBaseInput>(&mut ix_data).unwrap();
            #[derive(Debug)]
            pub struct SwapBaseInput {
                pub amount_in: u64,
                pub minimum_amount_out: u64,
            }
            impl From<instruction::SwapBaseInput> for SwapBaseInput {
                fn from(instr: instruction::SwapBaseInput) -> SwapBaseInput {
                    SwapBaseInput {
                        amount_in: instr.amount_in,
                        minimum_amount_out: instr.minimum_amount_out,
                    }
                }
            }
            println!("{:#?}", SwapBaseInput::from(ix));
        }
        instruction::SwapBaseOutput::DISCRIMINATOR => {
            let ix = decode_instruction::<instruction::SwapBaseOutput>(&mut ix_data).unwrap();
            #[derive(Debug)]
            pub struct SwapBaseOutput {
                pub max_amount_in: u64,
                pub amount_out: u64,
            }
            impl From<instruction::SwapBaseOutput> for SwapBaseOutput {
                fn from(instr: instruction::SwapBaseOutput) -> SwapBaseOutput {
                    SwapBaseOutput {
                        max_amount_in: instr.max_amount_in,
                        amount_out: instr.amount_out,
                    }
                }
            }
            println!("{:#?}", SwapBaseOutput::from(ix));
        }
        _ => {
            println!("unknow instruction: {}", instr_data);
        }
    }
    Ok(())
}

fn decode_instruction<T: anchor_lang::AnchorDeserialize>(
    slice: &mut &[u8],
) -> Result<T, anchor_lang::error::ErrorCode> {
    let instruction: T = anchor_lang::AnchorDeserialize::deserialize(slice)
        .map_err(|_| anchor_lang::error::ErrorCode::InstructionDidNotDeserialize)?;
    Ok(instruction)
}
