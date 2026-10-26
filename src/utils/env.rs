//! Loads and validates application configuration from the environment.

use std::env;

/// Required startup settings loaded from environment variables.
pub struct Env {
    /// Username used to bootstrap an administrator when no accounts exist.
    pub admin_username: String,
    /// Plaintext bootstrap password, hashed before storing the initial account.
    pub admin_password: String,
    /// Address on which the HTTP listener binds.
    pub host: String,
    /// TCP port for the HTTP listener.
    pub port: u16,
    /// Filesystem path of the SQLite database.
    pub db_path: String,
}

impl Env {
    /// Loads an optional `.env` file and reads the required administrator, listener, and database settings.
    ///
    /// # Errors
    ///
    /// Returns a message naming a missing environment variable, or rejects a port that cannot be parsed as a `u16`.
    pub fn load() -> Result<Self, String> {
        dotenvy::dotenv().ok();

        let admin_username =
            env::var("ADMIN_USERNAME").map_err(|_| "ADMIN_USERNAME is not set".to_string())?;

        let admin_password =
            env::var("ADMIN_PASSWORD").map_err(|_| "ADMIN_PASSWORD is not set".to_string())?;

        let host = env::var("HOST").map_err(|_| "HOST is not set".to_string())?;

        let port = env::var("PORT")
            .map_err(|_| "PORT is not set".to_string())?
            .parse::<u16>()
            .map_err(|_| "PORT must be a valid u16".to_string())?;

        let db_path = env::var("DB_PATH").map_err(|_| "DB_PATH is not set".to_string())?;

        Ok(Self {
            admin_username,
            admin_password,
            host,
            port,
            db_path,
        })
    }
}
