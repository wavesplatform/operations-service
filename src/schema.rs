// @generated automatically by Diesel CLI.

pub mod sql_types {
    #[derive(diesel::sql_types::SqlType, diesel::query_builder::QueryId)]
    #[diesel(postgres_type(name = "operation_type"))]
    pub struct OperationType;
}

diesel::table! {
    blocks_microblocks (id) {
        uid -> Int8,
        id -> Varchar,
        height -> Int4,
        time_stamp -> Int8,
    }
}

diesel::table! {
    use diesel::sql_types::*;
    use super::sql_types::OperationType;

    transactions (id) {
        uid -> Int8,
        id -> Varchar,
        block_uid -> Int8,
        sender -> Varchar,
        tx_type -> Int2,
        op_type -> OperationType,
        operation -> Jsonb,
    }
}

diesel::allow_tables_to_appear_in_same_query!(
    blocks_microblocks,
    transactions,
);
