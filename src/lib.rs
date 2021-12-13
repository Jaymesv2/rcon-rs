#![warn(missing_docs)]
//! # Rcon-rs
//! A crate for connecting to game servers using the source rcon protocol.
//! ```no_run
//! #[tokio::main]
//! async fn main() -> Result<(), rcon_rs::client::Error> {
//!     let mut c = rcon_rs::client::Connection::builder()
//!         .max_retries(3)
//!         .retry_delay(std::time::Duration::from_millis(1000))
//!         .exponential_backoff(true)
//!         .connect("127.0.0.1:22575", "password").await?;
//!     let response = c.cmd("a command".to_owned()).await?;
//!     println!("{}", response);
//!     Ok(())
//! }
//! ```

/// A [RCON](https://developer.valvesoftware.com/wiki/Source_RCON_Protocol) connection for interacting with remote servers.
#[cfg(feature = "client")]
pub mod client;
pub(crate) mod packet;
#[cfg(feature = "client")]
pub use client::Connection;
#[cfg(feature = "server")]
pub mod server;
#[cfg(feature = "server")]
pub use server::*;
