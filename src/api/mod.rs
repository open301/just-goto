#![allow(clippy::missing_docs_in_private_items)]

#[warn(clippy::missing_docs_in_private_items)]
pub mod docs;
#[warn(clippy::missing_docs_in_private_items)]
pub mod frontend;
#[warn(clippy::missing_docs_in_private_items)]
pub mod request;
pub mod routes;
#[warn(clippy::missing_docs_in_private_items)]
pub mod server;

#[cfg(test)]
mod tests;
