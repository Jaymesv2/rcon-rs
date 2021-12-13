#[warn(missing_docs)]

#[cfg(feature = "packet")]
pub mod packet;
#[cfg(feature = "packet")]
pub use packet::Packet;
/// A [RCON](https://developer.valvesoftware.com/wiki/Source_RCON_Protocol) connection for interacting with remote servers.
/// ```
/// #[tokio::main]
/// async fn main() -> std::io::Result<()> {
///     let mut c = rcon_rs::client::Connection::builder()
///         .max_retries(3)
///         .retry_delay(std::time::Duration::from_millis(1000))
///         .exponential_backoff(true)
///         .connect("127.0.0.1:22575", "password").await?;
///     c.run("a command".to_owned()).await?;
///     Ok(())
/// }
/// ```
#[cfg(feature = "client")]
pub mod client;
#[cfg(feature = "client")]
pub use client::Connection;
#[cfg(feature = "server")]
pub mod server;
#[cfg(feature = "server")]
pub use server::*;
