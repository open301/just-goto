//! JustGoto serves an embedded web interface and authenticated URL redirect management API.

mod api;
mod db;
mod utils;

use crate::utils::env::Env;
use crate::utils::startup::initialize_app;

/// Loads configuration, initializes persistent storage, and serves the application.
///
/// # Errors
///
/// Returns an error if configuration, database initialization, administrator creation, or the HTTP server fails.
///
/// # Panics
///
/// Panics if Tokio cannot initialize its runtime or a startup message cannot be written to standard output.
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Starting JustGoto...");

    // Load envs
    let env = Env::load()?;

    // Initialize the application
    initialize_app(&env.db_path, &env.admin_username, &env.admin_password)?;

    // Start the server
    api::server::start_server(&env.host, env.port, &env.db_path).await?;

    Ok(())
}
