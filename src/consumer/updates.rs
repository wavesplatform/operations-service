//! Blockchain updates

use anyhow::Error;
use async_trait::async_trait;
use tokio::sync::mpsc;

use crate::consumer::model::Transaction;

pub use self::updates_impl::BlockchainUpdates;

#[async_trait]
pub trait BlockchainUpdatesSource {
    async fn stream(self, from_height: u32) -> Result<mpsc::Receiver<BlockchainUpdate>, Error>;
}

#[derive(Debug)]
pub enum BlockchainUpdate {
    Append(AppendBlock),
    Rollback(Rollback),
}

#[derive(Debug)]
pub struct AppendBlock {
    pub block_id: String,
    pub height: u32,
    pub timestamp: Option<u64>,
    pub is_microblock: bool,
    pub transactions: Vec<Transaction>,
}

#[derive(Debug)]
pub struct Rollback {
    pub block_id: String,
}

mod updates_impl {
    use async_trait::async_trait;
    use tokio::{sync::mpsc, task};

    use waves_protobuf_schemas::waves::events::grpc::{
        blockchain_updates_api_client::BlockchainUpdatesApiClient, SubscribeEvent, SubscribeRequest,
    };

    use super::{BlockchainUpdate, BlockchainUpdatesSource};

    #[derive(Clone)]
    pub struct BlockchainUpdates(BlockchainUpdatesApiClient<tonic::transport::Channel>);

    impl BlockchainUpdates {
        pub async fn connect(blockchain_updates_url: String) -> Result<Self, anyhow::Error> {
            let grpc_client = BlockchainUpdatesApiClient::connect(blockchain_updates_url).await?;
            Ok(BlockchainUpdates(grpc_client))
        }
    }

    #[async_trait]
    impl BlockchainUpdatesSource for BlockchainUpdates {
        async fn stream(self, from_height: u32) -> Result<mpsc::Receiver<BlockchainUpdate>, anyhow::Error> {
            let BlockchainUpdates(mut grpc_client) = self;

            let request = tonic::Request::new(SubscribeRequest {
                from_height: from_height as i32,
                to_height: 0,
            });

            let stream = grpc_client.subscribe(request).await?.into_inner();

            let (tx, rx) = mpsc::channel::<BlockchainUpdate>(16); // Buffer size is arbitrary

            task::spawn(async move {
                let res = pump_messages(stream, tx).await;
                if let Err(err) = res {
                    log::error!("Error receiving blockchain updates: {}", err);
                } else {
                    log::warn!("GRPC connection closed by the server");
                }
            });

            async fn pump_messages(
                mut stream: tonic::Streaming<SubscribeEvent>,
                tx: mpsc::Sender<BlockchainUpdate>,
            ) -> anyhow::Result<()> {
                while let Some(event) = stream.message().await? {
                    if let Some(update) = event.update {
                        let update = convert::convert_update(update)?;
                        tx.send(update).await?;
                    }
                }
                Ok(())
            }

            Ok(rx)
        }
    }

    mod convert {
        use itertools::Itertools;
        use thiserror::Error;

        use waves_protobuf_schemas::waves::invoke_script_result::call::argument::Value;
        use waves_protobuf_schemas::waves::{
            events::{
                blockchain_updated::{
                    append::{BlockAppend, Body, MicroBlockAppend},
                    Append, Update,
                },
                transaction_metadata::{ethereum_metadata::Action, EthereumMetadata, InvokeScriptMetadata, Metadata},
                BlockchainUpdated, TransactionMetadata,
            },
            invoke_script_result::call::Argument,
            signed_transaction::Transaction as TransactionEnum,
            transaction::Data as WavesTxData,
            Amount as WavesAmount, Block, InvokeScriptTransactionData, MicroBlock, SignedMicroBlock, SignedTransaction,
            Transaction as WavesTransaction,
        };

        use super::super::{AppendBlock, BlockchainUpdate, Rollback};
        use crate::consumer::model::{Amount, Arg, Call, OperationType, Transaction, TransactionType};

        #[derive(Error, Debug)]
        #[error("failed to convert blockchain update: {0}")]
        pub(super) struct ConvertError(&'static str);

        pub(super) fn convert_update(src: BlockchainUpdated) -> Result<BlockchainUpdate, ConvertError> {
            let height = src.height as u32;
            let update = src.update;
            match update {
                Some(Update::Append(append)) => {
                    let body = append.body.ok_or(ConvertError("append body is None"))?;
                    let Append {
                        transaction_ids,
                        transactions_metadata,
                        ..
                    } = append;
                    let is_microblock =
                        extract_is_microblock(&body).ok_or(ConvertError("failed to extract is_microblock"))?;
                    let id = extract_id(&body, &src.id).ok_or(ConvertError("failed to extract block id"))?;
                    let id = base58(id);
                    let timestamp = extract_timestamp(&body);
                    let transactions = extract_transactions(body).ok_or(ConvertError("transactions is None"))?;
                    assert!(
                        transaction_ids.len() == transactions.len()
                            && transactions.len() == transactions_metadata.len()
                    );
                    let block_info = BlockInfo { height, timestamp };
                    let transactions =
                        convert_transactions(transaction_ids, transactions, transactions_metadata, block_info)?;
                    let append = AppendBlock {
                        block_id: id,
                        height,
                        timestamp,
                        is_microblock,
                        transactions,
                    };
                    Ok(BlockchainUpdate::Append(append))
                }
                Some(Update::Rollback(_)) => {
                    let rollback_to_block_id = base58(&src.id);
                    let rollback = Rollback {
                        block_id: rollback_to_block_id,
                    };
                    Ok(BlockchainUpdate::Rollback(rollback))
                }
                _ => Err(ConvertError("failed to parse blockchain update")),
            }
        }

        fn extract_is_microblock(body: &Body) -> Option<bool> {
            match body {
                Body::Block(BlockAppend { block: Some(_), .. }) => Some(false),
                Body::MicroBlock(MicroBlockAppend {
                    micro_block: Some(_), ..
                }) => Some(true),
                _ => None,
            }
        }

        fn extract_id<'a>(body: &'a Body, block_id: &'a Vec<u8>) -> Option<&'a Vec<u8>> {
            match body {
                Body::Block(_) => Some(block_id),
                Body::MicroBlock(MicroBlockAppend {
                    micro_block: Some(SignedMicroBlock { total_block_id, .. }),
                    ..
                }) => Some(total_block_id),
                _ => None,
            }
        }

        fn extract_timestamp(body: &Body) -> Option<u64> {
            if let Body::Block(BlockAppend {
                block:
                    Some(Block {
                        header: Some(ref header),
                        ..
                    }),
                ..
            }) = body
            {
                Some(header.timestamp as u64)
            } else {
                None
            }
        }

        fn extract_transactions(body: Body) -> Option<Vec<SignedTransaction>> {
            match body {
                Body::Block(BlockAppend {
                    block: Some(Block { transactions, .. }),
                    ..
                }) => Some(transactions),
                Body::MicroBlock(MicroBlockAppend {
                    micro_block:
                        Some(SignedMicroBlock {
                            micro_block: Some(MicroBlock { transactions, .. }),
                            ..
                        }),
                    ..
                }) => Some(transactions),
                _ => None,
            }
        }

        struct BlockInfo {
            height: u32,
            #[allow(dead_code)]
            timestamp: Option<u64>, // Not usable, only present for full blocks
        }

        fn convert_transactions(
            transaction_ids: Vec<Vec<u8>>,
            transactions: Vec<SignedTransaction>,
            transactions_metadata: Vec<TransactionMetadata>,
            block_info: BlockInfo,
        ) -> Result<Vec<Transaction>, ConvertError> {
            let ids = transaction_ids.into_iter();
            let txs = transactions.into_iter();
            let met = transactions_metadata.into_iter();
            let iter = ids.zip(txs).zip(met);
            iter.filter_map(|((id, tx), meta)| convert_tx(id, tx, meta, &block_info).transpose())
                .collect()
        }

        fn convert_tx(
            id: Vec<u8>,
            tx: SignedTransaction,
            meta: TransactionMetadata,
            block_info: &BlockInfo,
        ) -> Result<Option<Transaction>, ConvertError> {
            let tx = match extract_op_type(&meta) {
                Some(op_type @ OperationType::InvokeScript) => {
                    let tx_type = extract_tx_type(&meta).ok_or(ConvertError("missing tx type"))?;
                    let tx_data = extract_transaction_data(&tx, &meta).ok_or(ConvertError("missing tx data"))?;
                    let invoke_script_data = extract_invoke_script_data(&tx, &meta)?;
                    Transaction {
                        id: base58(&id),
                        op_type,
                        tx_type,
                        height: block_info.height,
                        timestamp: tx_data.get_timestamp(),
                        //block_timestamp: block_info.timestamp.unwrap_or_default(), //TODO unusable
                        fee: tx_data.get_fee().ok_or(ConvertError("fee"))?,
                        sender: base58(&meta.sender_address),
                        sender_public_key: base58(tx_data.get_sender_public_key()),
                        proofs: tx.proofs.iter().map(|p| base58(p)).collect_vec(),
                        dapp: base58(&invoke_script_data.meta.d_app_address),
                        payment: invoke_script_data.get_payments(),
                        call: invoke_script_data.get_call()?,
                    }
                }
                None => return Ok(None),
            };

            Ok(Some(tx))
        }

        fn extract_op_type(meta: &TransactionMetadata) -> Option<OperationType> {
            match meta.metadata {
                Some(Metadata::InvokeScript(_)) => Some(OperationType::InvokeScript),
                Some(Metadata::Ethereum(EthereumMetadata {
                    action: Some(Action::Invoke(_)),
                    ..
                })) => Some(OperationType::InvokeScript),
                _ => None,
            }
        }

        fn extract_tx_type(meta: &TransactionMetadata) -> Option<TransactionType> {
            match meta.metadata {
                Some(Metadata::InvokeScript(_)) => Some(TransactionType::InvokeScript),
                Some(Metadata::Ethereum(EthereumMetadata {
                    action: Some(Action::Invoke(_)),
                    ..
                })) => Some(TransactionType::EthereumTransaction),
                _ => None,
            }
        }

        fn extract_transaction_data<'a>(
            tx: &'a SignedTransaction,
            meta: &'a TransactionMetadata,
        ) -> Option<TransactionData<'a>> {
            match (&tx.transaction, &meta.metadata) {
                (Some(TransactionEnum::WavesTransaction(tx)), _) => Some(TransactionData::Waves(tx)),
                (Some(TransactionEnum::EthereumTransaction(_)), Some(Metadata::Ethereum(meta))) => {
                    Some(TransactionData::Ethereum(meta))
                }
                _ => None,
            }
        }

        fn extract_invoke_script_data<'a>(
            tx: &'a SignedTransaction,
            meta: &'a TransactionMetadata,
        ) -> Result<InvokeScriptData<'a>, ConvertError> {
            let waves_data = match &tx.transaction {
                Some(TransactionEnum::WavesTransaction(WavesTransaction {
                    data: Some(WavesTxData::InvokeScript(data)),
                    ..
                })) => Some(data),
                Some(TransactionEnum::EthereumTransaction(_)) => None,
                _ => return Err(ConvertError("unexpected InvokeScript transaction contents")),
            };

            let meta = match &meta.metadata {
                Some(Metadata::InvokeScript(meta)) => meta,
                Some(Metadata::Ethereum(EthereumMetadata {
                    action: Some(Action::Invoke(meta)),
                    ..
                })) => meta,
                _ => return Err(ConvertError("unexpected InvokeScript metadata contents")),
            };

            Ok(InvokeScriptData { waves_data, meta })
        }

        enum TransactionData<'a> {
            Waves(&'a WavesTransaction),
            Ethereum(&'a EthereumMetadata),
        }

        struct InvokeScriptData<'a> {
            waves_data: Option<&'a InvokeScriptTransactionData>,
            meta: &'a InvokeScriptMetadata,
        }

        impl TransactionData<'_> {
            fn get_fee(&self) -> Option<Amount> {
                match self {
                    TransactionData::Waves(wtx) => wtx.fee.as_ref().map(convert_amount),
                    TransactionData::Ethereum(etx) => Some(Amount::new(etx.fee, None)),
                }
            }

            fn get_sender_public_key(&self) -> &Vec<u8> {
                match self {
                    TransactionData::Waves(wtx) => &wtx.sender_public_key,
                    TransactionData::Ethereum(etx) => &etx.sender_public_key,
                }
            }

            fn get_timestamp(&self) -> u64 {
                match self {
                    TransactionData::Waves(wtx) => wtx.timestamp as u64,
                    TransactionData::Ethereum(etx) => etx.timestamp as u64,
                }
            }
        }

        impl InvokeScriptData<'_> {
            fn get_payments(&self) -> Vec<Amount> {
                let payments = if let Some(data) = self.waves_data {
                    assert_eq!(data.payments, self.meta.payments);
                    &data.payments
                } else {
                    &self.meta.payments
                };
                payments.iter().map(|p| convert_amount(p)).collect_vec()
            }

            fn get_call(&self) -> Result<Call, ConvertError> {
                let function = self.meta.function_name.clone();
                let args = convert_args(&self.meta.arguments)?;

                fn convert_args(args: &Vec<Argument>) -> Result<Vec<Arg>, ConvertError> {
                    args.iter()
                        .map(|arg| {
                            arg.value
                                .as_ref()
                                .ok_or(ConvertError("missing argument"))
                                .map(|arg| match arg {
                                    Value::IntegerValue(v) => Ok(Arg::Integer(*v)),
                                    Value::BinaryValue(v) => Ok(Arg::Binary(base58(v))),
                                    Value::StringValue(v) => Ok(Arg::String(v.to_owned())),
                                    Value::BooleanValue(v) => Ok(Arg::Boolean(*v)),
                                    Value::CaseObj(v) => Ok(Arg::CaseObj(base58(v))),
                                    Value::List(vv) => convert_args(&vv.items).map(|list| Arg::List(list)),
                                })
                                .and_then(|r| r)
                        })
                        .collect()
                }

                Ok(Call { function, args })
            }
        }

        fn convert_amount(a: &WavesAmount) -> Amount {
            let amount = a.amount;
            let asset_id = if a.asset_id.is_empty() {
                None
            } else {
                Some(base58(&a.asset_id))
            };
            Amount::new(amount, asset_id)
        }

        fn base58(bytes: &[u8]) -> String {
            bs58::encode(bytes).into_string()
        }
    }
}