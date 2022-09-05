//! Operations service application library.

#[macro_use]
extern crate diesel;
extern crate builder_pattern as builder;
extern crate wavesexchange_log as log;
extern crate wavesexchange_warp as wx_warp;

mod schema;

pub mod common;
pub mod consumer;
pub mod service;
