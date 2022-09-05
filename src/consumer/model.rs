//! Transaction data model, serializable to JSON

use serde::Serialize;
use serde_repr::Serialize_repr;

#[derive(Serialize, Debug)]
#[serde(rename_all = "snake_case")]
pub struct Transaction {
    pub id: String,
    #[serde(rename = "type")]
    pub op_type: OperationType,
    #[serde(rename = "origin_transaction_type")]
    pub tx_type: TransactionType,
    pub height: u32,
    pub timestamp: u64,
    //pub block_timestamp: u64, // Can't reliably get it without redesign
    pub fee: Amount,
    pub sender: String,
    pub sender_public_key: String,
    pub proofs: Vec<String>,
    pub dapp: String,
    pub payment: Vec<Amount>,
    pub call: Call,
}

#[derive(Copy, Clone, Serialize, Debug)]
#[serde(rename_all = "snake_case")]
pub enum OperationType {
    InvokeScript,
}

#[repr(u8)]
#[derive(Copy, Clone, Serialize_repr, Debug)]
pub enum TransactionType {
    InvokeScript = 16,
    EthereumTransaction = 18,
}

#[derive(Serialize, Debug)]
pub struct Amount {
    #[serde(rename = "amount")]
    pub amount: i64,

    #[serde(rename = "id")]
    pub asset_id: String,
}

impl Amount {
    const WAVES_ASSET_ID: &'static str = "WAVES";

    pub fn new(amount: i64, asset_id: Option<String>) -> Self {
        Amount {
            amount,
            asset_id: asset_id.unwrap_or_else(|| Self::WAVES_ASSET_ID.to_owned()),
        }
    }
}

#[derive(Serialize, Debug)]
pub struct Call {
    pub function: String,
    pub args: Vec<Arg>,
}

#[derive(Serialize, Debug)]
#[serde(tag = "type", content = "value")]
#[serde(rename_all = "snake_case")]
pub enum Arg {
    Integer(i64),
    Binary(String),
    String(String),
    Boolean(bool),
    CaseObj(String),
    List(Vec<Arg>),
}
