//! Common code between consumer & web-service.

pub mod database {
    pub mod config {
        use serde::Deserialize;
        use thiserror::Error;

        #[derive(Deserialize, Clone)]
        pub struct PostgresConfig {
            #[serde(rename = "pghost")]
            pub host: String,

            #[serde(rename = "pgport", default = "default_pgport")]
            pub port: u16,

            #[serde(rename = "pgdatabase")]
            pub database: String,

            #[serde(rename = "pguser")]
            pub user: String,

            #[serde(rename = "pgpassword")]
            pub password: String,
        }

        fn default_pgport() -> u16 {
            5432
        }

        #[derive(Error, Debug)]
        #[error("database config error: {0}")]
        pub struct DbConfigError(#[from] pub envy::Error);

        pub fn load() -> Result<PostgresConfig, DbConfigError> {
            let pg_config = envy::from_env::<PostgresConfig>()?;
            Ok(pg_config)
        }

        impl PostgresConfig {
            pub fn database_url(&self) -> String {
                format!(
                    "postgres://{}:{}@{}:{}/{}",
                    self.user, self.password, self.host, self.port, self.database
                )
            }
        }

        mod format {
            use std::fmt;

            impl fmt::Debug for super::PostgresConfig {
                fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                    // Intentionally avoid printing password for security reasons
                    write!(
                        f,
                        "Postgres(server={}:{}; database={}; user={})",
                        self.host, self.port, self.database, self.user
                    )
                }
            }
        }
    }

    pub mod types {
        use diesel_derive_enum::DbEnum;

        #[derive(DbEnum, Debug)]
        #[ExistingTypePath = "crate::schema::sql_types::OperationType"]
        pub enum OperationType {
            InvokeScript,
        }
    }
}
