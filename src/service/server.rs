//! Operations Web server

use std::sync::Arc;

use warp::Filter;
use wavesexchange_warp::endpoints::{livez, readyz, startz};

use crate::service::repo::Repo;

pub use self::builder::ServerBuilder;

/// The web server
pub struct Server<R: Repo> {
    repo: Arc<R>,
}

mod builder {
    use std::sync::Arc;

    use builder::Builder;

    use super::Server;
    use crate::service::repo::Repo;

    #[derive(Builder)]
    pub struct ServerBuilder<R: Repo> {
        #[public]
        repo: R,
    }

    impl<R: Repo> ServerBuilder<R> {
        pub fn new_server(self) -> Server<R> {
            Server {
                repo: Arc::new(self.repo),
            }
        }
    }
}

impl<R> Server<R>
where
    Self: Send + Sync + 'static,
    R: Repo + Sync + Send,
{
    pub async fn run(self: Arc<Self>, port: u16) {
        let with_self = warp::any().map(move || self.clone());

        let get_operations = warp::any()
            .and(with_self.clone())
            .and(warp::path!("operations"))
            .and(warp::get())
            .and(warp::query::<endpoints::OperationsQuery>())
            .and_then(Self::get_operations_handler)
            .recover(error_handling::error_handler);

        let routes = livez()
            .or(readyz())
            .or(startz())
            .or(get_operations)
            .recover(error_handling::handle_rejection)
            .with(warp::filters::log::log("operations::server::access"));

        warp::serve(routes).run(([0, 0, 0, 0], port)).await;
    }
}

mod endpoints {
    use itertools::Itertools;
    use std::sync::Arc;

    use serde::{Deserialize, Serialize};
    use thiserror::Error;
    use warp::{http::StatusCode, reject::Reject, Rejection, Reply};
    use wx_warp::pagination::{List, PageInfo};

    use super::Server;
    use crate::common::database::types::OperationType;
    use crate::service::repo::{Operation, Page, Repo};

    const MAX_QUERY_LIMIT: u32 = 100;

    /// Query parameters for the GET `/operations` endpoint.
    #[derive(Deserialize)]
    pub(super) struct OperationsQuery {
        /// Sender's address of the transaction
        #[serde(rename = "sender")]
        sender: Option<String>,

        /// Filter by operation type
        #[serde(rename = "type__in")]
        types: Option<Vec<OpType>>,

        /// Max value is `100`
        #[serde(rename = "limit")]
        limit: Option<u32>,

        /// Contents of the `page_info/last_cursor` field of the previous response
        #[serde(rename = "after")]
        after: Option<String>,
    }

    #[derive(Copy, Clone, PartialEq, Eq, Hash, Deserialize)]
    #[serde(rename_all = "snake_case")]
    pub(super) enum OpType {
        #[serde(rename = "invoke_script")]
        InvokeScript,
    }

    /// Response for the GET `/operations` endpoint, encoded as JSON.
    #[derive(Serialize)]
    struct OperationsResponse<TxUID: Serialize> {
        #[serde(flatten)]
        list: List<Operation<TxUID>>,
    }

    impl<R: Repo> Server<R> {
        /// Handler for the GET `/operations` endpoint.
        pub(super) async fn get_operations_handler(
            self: Arc<Self>,
            query: OperationsQuery,
        ) -> Result<impl Reply, Rejection> {
            if let Some(limit) = query.limit {
                if limit > MAX_QUERY_LIMIT {
                    return Err(GetOperationsError::InvalidLimit.into());
                }
            }

            let types = query.types.map(|list| {
                list.iter()
                    .map(|t| match t {
                        OpType::InvokeScript => OperationType::InvokeScript,
                    })
                    .collect_vec()
            });
            let sender = query.sender;
            let start = query
                .after
                .map(|v| v.parse().map_err(|_| GetOperationsError::InvalidAfter))
                .transpose()?;
            let page = Page {
                start,
                limit: query.limit.unwrap_or(MAX_QUERY_LIMIT),
            };

            // Fetch transactions from the database
            let repo = self.repo.clone();
            let (list, next) = repo
                .fetch_operations(types, sender, page)
                .await
                .map_err(|e| GetOperationsError::ServerError(e))?;
            log::debug!("fetched {} operations", list.len());

            let res = OperationsResponse {
                list: List {
                    page_info: PageInfo {
                        has_next_page: next.is_some(),
                        last_cursor: next.map(|v| v.to_string()),
                    },
                    items: list,
                },
            };

            let json = warp::reply::json(&res);
            let reply = warp::reply::with_status(json, StatusCode::OK);

            Ok(reply)
        }
    }

    #[derive(Error, Debug)]
    pub enum GetOperationsError {
        #[error("Bad request: invalid 'after'")]
        InvalidAfter,
        #[error("Bad request: invalid 'limit'")]
        InvalidLimit,
        #[error("Internal server error")]
        ServerError(anyhow::Error),
    }

    impl Reject for GetOperationsError {}

    impl GetOperationsError {
        pub fn status_code(&self) -> StatusCode {
            match self {
                GetOperationsError::InvalidAfter => StatusCode::BAD_REQUEST,
                GetOperationsError::InvalidLimit => StatusCode::BAD_REQUEST,
                GetOperationsError::ServerError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            }
        }
    }
}

mod error_handling {
    use std::convert::Infallible;

    use warp::{http::StatusCode, Rejection, Reply};

    use super::endpoints::GetOperationsError;

    pub(super) async fn error_handler(err: Rejection) -> Result<impl Reply, Rejection> {
        if let Some(ops_error) = err.find::<GetOperationsError>() {
            if let GetOperationsError::ServerError(e) = ops_error {
                log::error!("Internal error: {:?}", e);
            }
            let error_text = ops_error.to_string();
            let code = ops_error.status_code();
            let resp = warp::reply::with_status(error_text, code);
            Ok(resp)
        } else {
            Err(err)
        }
    }

    pub(super) async fn handle_rejection(err: Rejection) -> Result<impl Reply, Infallible> {
        let (code, message) = if err.is_not_found() {
            (StatusCode::NOT_FOUND, "Not Found")
        } else if err.find::<warp::reject::MethodNotAllowed>().is_some() {
            (StatusCode::METHOD_NOT_ALLOWED, "Method Not Allowed")
        } else if err.find::<warp::reject::InvalidQuery>().is_some() {
            (StatusCode::BAD_REQUEST, "Bad request: invalid query")
        } else {
            log::error!("Unhandled error: {:?}", err);
            (StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error")
        };

        Ok(warp::reply::with_status(message, code))
    }
}
