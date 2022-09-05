//! Consumer's storage

use anyhow::Result;
use async_trait::async_trait;

pub use self::postgres_storage::PostgresStorage;

#[async_trait]
pub trait Storage {
    type Repo: Repo;

    /// Execute the given function within a database transaction.
    async fn transaction<F, R>(&self, f: F) -> Result<R>
    where
        F: FnOnce(&mut Self::Repo) -> Result<R>,
        F: Send + 'static,
        R: Send + 'static;
}

pub trait Repo {
    type BlockUID: Copy;

    fn last_height(&mut self) -> Result<Option<u32>>;
    fn rollback_to_height(&mut self, height: u32) -> Result<()>;
    fn rollback_to_block(&mut self, block_uid: Self::BlockUID) -> Result<()>;
    fn insert_block(&mut self, id: &str, height: u32, timestamp: u64) -> Result<Self::BlockUID>;
    fn insert_tx(
        &mut self,
        id: &str,
        block_uid: Self::BlockUID,
        sender: &str,
        tx_type: u8,
        operation: serde_json::Value,
    ) -> Result<()>;
    fn block_uid(&mut self, block_id: &str) -> Result<Self::BlockUID>;
}

mod postgres_storage {
    use std::sync::{Arc, Mutex};

    use anyhow::Result;
    use async_trait::async_trait;
    use diesel::{dsl::max, ExpressionMethods, QueryDsl, RunQueryDsl};
    use diesel::{pg::PgConnection, Connection};
    use tokio::task;

    use super::{Repo, Storage};
    use crate::common::database::types::OperationType;
    use crate::schema::{blocks_microblocks, transactions};

    #[derive(Clone)]
    pub struct PostgresStorage {
        conn: Arc<Mutex<Option<Box<PgConnection>>>>,
    }

    impl PostgresStorage {
        pub fn new(conn: PgConnection) -> Self {
            PostgresStorage {
                conn: Arc::new(Mutex::new(Some(Box::new(conn)))),
            }
        }
    }

    #[async_trait]
    impl Storage for PostgresStorage {
        type Repo = PgConnection;

        async fn transaction<F, R>(&self, f: F) -> Result<R>
        where
            F: FnOnce(&mut Self::Repo) -> Result<R>,
            F: Send + 'static,
            R: Send + 'static,
        {
            let conn_arc = self.conn.clone();
            task::spawn_blocking(move || {
                let mut conn_guard = conn_arc.lock().unwrap();
                let mut conn = conn_guard.take().expect("connection is gone");
                let result = conn.transaction(|conn| f(conn));
                *conn_guard = Some(conn);
                result
            })
            .await
            .expect("sync task panicked")
        }
    }

    impl Repo for PgConnection {
        type BlockUID = i64;

        fn last_height(&mut self) -> Result<Option<u32>> {
            log::timer!("last_height()", level = trace);
            let height: Option<i32> = blocks_microblocks::table
                .select(max(blocks_microblocks::height))
                .first(self)?;
            Ok(height.map(|h| h as u32))
        }

        fn rollback_to_height(&mut self, height: u32) -> Result<()> {
            log::timer!("rollback_to_height()", level = trace);
            let _row_count =
                diesel::delete(blocks_microblocks::table.filter(blocks_microblocks::height.gt(height as i32)))
                    .execute(self)?;
            Ok(())
        }

        fn rollback_to_block(&mut self, block_uid: Self::BlockUID) -> Result<()> {
            log::timer!("rollback_to_block()", level = trace);
            let _row_count = diesel::delete(blocks_microblocks::table.filter(blocks_microblocks::uid.gt(block_uid)))
                .execute(self)?;
            Ok(())
        }

        fn insert_block(&mut self, id: &str, height: u32, timestamp: u64) -> Result<Self::BlockUID> {
            log::timer!("insert_block()", level = trace);
            let values = (
                blocks_microblocks::id.eq(id),
                blocks_microblocks::height.eq(height as i32),
                blocks_microblocks::time_stamp.eq(timestamp as i64),
            );
            let res = diesel::insert_into(blocks_microblocks::table)
                .values(&values)
                .returning(blocks_microblocks::uid)
                .get_results(self)?;
            assert_eq!(res.len(), 1);
            Ok(res[0])
        }

        fn insert_tx(
            &mut self,
            id: &str,
            block_uid: Self::BlockUID,
            sender: &str,
            tx_type: u8,
            operation: serde_json::Value,
        ) -> Result<()> {
            log::timer!("insert_tx()", level = trace);
            let values = (
                transactions::id.eq(id),
                transactions::block_uid.eq(block_uid),
                transactions::sender.eq(sender),
                transactions::tx_type.eq(tx_type as i16),
                transactions::op_type.eq(OperationType::InvokeScript),
                transactions::operation.eq(operation),
            );
            let row_count = diesel::insert_into(transactions::table).values(&values).execute(self)?;
            assert_eq!(row_count, 1);
            Ok(())
        }

        fn block_uid(&mut self, block_id: &str) -> Result<Self::BlockUID> {
            log::timer!("block_uid()", level = trace);
            let res = blocks_microblocks::table
                .select(blocks_microblocks::uid)
                .filter(blocks_microblocks::id.eq(block_id))
                .get_result(self)?;
            Ok(res)
        }
    }
}
