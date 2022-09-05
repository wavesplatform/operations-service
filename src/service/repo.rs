//! Operations service's repo

use std::str::FromStr;

use async_trait::async_trait;
use serde::Serialize;

use crate::common::database::types::OperationType;

#[async_trait]
pub trait Repo {
    type TxUID: Copy + Send + FromStr + ToString + Serialize;

    async fn fetch_operations(
        &self,
        op_types: Option<Vec<OperationType>>,
        sender: Option<String>,
        page: Page<Self::TxUID>,
    ) -> anyhow::Result<(Vec<Operation<Self::TxUID>>, Option<Self::TxUID>)>;
}

#[derive(Serialize, Queryable)]
pub struct Operation<TxUID> {
    #[serde(skip)]
    tx_uid: TxUID,
    #[serde(flatten)]
    body: serde_json::Value,
}

pub struct Page<TxUID> {
    pub start: Option<TxUID>,
    pub limit: u32,
}

pub mod postgres {
    use async_trait::async_trait;
    use diesel::{prelude::*, QueryDsl};

    use super::Repo;
    use super::{Operation, OperationType, Page};
    use crate::schema::transactions;
    use crate::service::db::pool::PgPool;

    pub struct PgRepo {
        pgpool: PgPool,
    }

    impl PgRepo {
        pub fn new(pgpool: PgPool) -> Self {
            PgRepo { pgpool }
        }
    }

    #[async_trait]
    impl Repo for PgRepo {
        type TxUID = i64;

        async fn fetch_operations(
            &self,
            op_types: Option<Vec<OperationType>>,
            sender: Option<String>,
            page: Page<Self::TxUID>,
        ) -> anyhow::Result<(Vec<Operation<Self::TxUID>>, Option<Self::TxUID>)> {
            log::timer!("fetch_operations()");
            let conn = self.pgpool.get().await?;
            let mut res = conn
                .interact(move |conn| {
                    let mut query = transactions::table
                        .select((transactions::uid, transactions::operation))
                        .into_boxed();

                    if let Some(op_types) = op_types {
                        if !op_types.is_empty() {
                            query = query.filter(transactions::op_type.eq_any(op_types));
                        }
                    }

                    if let Some(sender) = sender {
                        query = query.filter(transactions::sender.eq(sender));
                    }

                    if let Some(from_uid) = page.start {
                        query = query.filter(transactions::uid.ge(from_uid));
                    }

                    query = query.limit((page.limit + 1) as i64);

                    query = query.order(transactions::uid);

                    query.load::<Operation<i64>>(conn)
                })
                .await
                .map_err(|e| anyhow::anyhow!("{}", e))?
                .map_err(|e| anyhow::anyhow!("{}", e))?;
            let page = if res.len() > page.limit as usize {
                let last = res.pop().expect("extra item");
                Some(last.tx_uid)
            } else {
                None
            };
            Ok((res, page))
        }
    }
}
