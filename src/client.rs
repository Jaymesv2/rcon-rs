use futures::{SinkExt, StreamExt};
use log::*;
use rand::{thread_rng, Rng};
use std::io::{self, Error, ErrorKind};
use tokio::net::{TcpStream, ToSocketAddrs, lookup_host};
use std::net::SocketAddr;
use tokio_util::codec::*;
use std::time::Duration;
use tokio::time::sleep;
use super::packet::*;

/// A [RCON](https://developer.valvesoftware.com/wiki/Source_RCON_Protocol) Connection.
/// Automatic retries to connect to the server before returning an error.
pub struct Connection {
    stream: Option<Framed<TcpStream, PacketCodec>>,
    host: SocketAddr,
    password: String,
    authenticated: bool,
    max_retries: u32,
    retry_delay: Duration,
    exponential_backoff: bool,
}

/// A builder for the connection struct.
pub struct Builder {
    max_retries: u32,
    retry_delay: Duration,
    exponential_backoff: bool,
}

impl Builder {
    /// Completes the builder and connects to the rcon server using the specified options by the builder.
    /// Eargerly connects to the server.
    pub async fn connect<S: ToSocketAddrs, P: ToString>(self, addr: S, password: P) -> io::Result<Connection> {
        let addr = match lookup_host(addr).await?.next() {
            Some(s) => s,
            None => return Err(io::Error::new(io::ErrorKind::NotFound, "unable to resolve host")),
        };

        let mut c = Connection {
            stream: None,
            host: addr,
            password: password.to_string(),
            authenticated: false,
            max_retries: self.max_retries,
            retry_delay: self.retry_delay,
            exponential_backoff: self.exponential_backoff,
        };

        c.connect().await?;
        c.login().await?;
        Ok(c)
    }

    /// Sets the maximum number of retries that will be made when calling `Connection::run` before throwing an error.
    pub fn max_retries(mut self, retries: u32) -> Self {
        self.max_retries = retries;
        self
    }

    /// Sets the delay between retries.
    pub fn retry_delay(mut self, retry_delay: Duration) -> Self {
        self.retry_delay = retry_delay;
        self
    }

    /// Sets whether the exponential backoff will be used when trying to reconnect.
    pub fn exponential_backoff(mut self, exponential_backoff: bool) -> Self {
        self.exponential_backoff = exponential_backoff;
        self
    }
}

impl Connection {
    /// Creates a `Builder` for `Connection`.
    pub fn builder() -> Builder {
        Builder {
            max_retries: 3,
            retry_delay: Duration::from_millis(1000),
            exponential_backoff: false,
        }
    }
    
    /// Sends a command to the connected server. 
    pub async fn cmd(&mut self, cmd: String) -> io::Result<String> {
        debug!("running command: \"{}\"", &cmd);
        let pk = Packet {
            ptype: PacketType::ExecCommand,
            id: thread_rng().gen::<i32>(),
            body: cmd,
        };
        let stream = if let Some(s) = self.stream.as_mut() {
            s
        } else {
            trace!("reconnecting");
            self.connect().await?;
            self.login().await?;
            self.stream.as_mut().unwrap()
        };

        stream.send(pk).await?;

        let p = match stream.next().await {
            Some(Ok(x)) => Ok(x),
            Some(Err(e)) => Err(e),
            None => Err(Error::new(
                ErrorKind::ConnectionAborted,
                "Server ended the connection",
            )),
        }?;
        Ok(p.body)
    }
}

// private methods
impl Connection {
    async fn connect(&mut self) -> io::Result<()> {
        for retries in 1..self.max_retries+1 {
            let s = match TcpStream::connect(self.host).await {
                Ok(s) => s,
                Err(e) => {
                    trace!("failed to connect to server: {}", e);
                    continue;
                }
            };

            self.stream = Some(Framed::new(s, PacketCodec::new_client()));
            sleep(if self.exponential_backoff {
                Duration::from_millis((self.retry_delay.as_millis() as u64).pow(retries ) )
            } else {
                self.retry_delay
            }).await;
            
            return Ok(())
        }
        Err(io::Error::new(io::ErrorKind::NotFound, "unable to resolve host"))
    }

    async fn login(&mut self) -> io::Result<()> {
        self.authenticated = false;
        let aid = thread_rng().gen::<i32>();

        let pk = Packet {
            ptype: PacketType::Auth,
            id: aid,
            body: self.password.clone(),
        };

        let stream = self.stream.as_mut().ok_or_else(|| io::Error::new(io::ErrorKind::NotConnected, "Not connected"))?;

        stream.send(pk).await?;

        for _ in 0..2 {
            match stream.next().await {
                Some(Ok(p)) if p.ptype == PacketType::AuthResponse => {
                    return if p.id == aid {
                        trace!("client successfully logged in");
                        self.authenticated = true;
                        Ok(())
                    } else {
                        trace!("authentication failed");
                        Err(io::Error::new(io::ErrorKind::Other, "Incorrect password"))
                    };
                }
                Some(Ok(_)) => {
                    trace!("client recieved non auth response when reading for auth response");
                    continue;
                }
                Some(Err(e)) => {
                    trace!("read error from buffer: {:?}", e);
                    continue;
                } // fix this
                None => {
                    trace!("stream ended while waiting for auth response");
                    return Err(io::Error::new(io::ErrorKind::ConnectionAborted, ""));
                }
            }
        };
        Err(io::Error::new(io::ErrorKind::InvalidData, "Recieved Invalid data from server"))
    }

    
}
