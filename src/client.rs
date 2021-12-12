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

pub struct Connection {
    stream: Option<Framed<TcpStream, PacketCodec>>,
    host: SocketAddr,
    password: String,
    authenticated: bool,
    max_retries: u32,
    /// None = exponential backoff
    /// in milliseconds
    retry_delay: Duration,
    exponential_backoff: bool,
}

pub struct Builder {
    max_retries: u32,
    retry_delay: Duration,
    exponential_backoff: bool,
}

impl Builder {
    pub async fn connect<S: ToSocketAddrs>(self, addr: S, password: String) -> io::Result<Connection> {
        let addr = match lookup_host(addr).await?.next() {
            Some(s) => s,
            None => return Err(io::Error::new(io::ErrorKind::NotFound, "unable to resolve host")),
        };

        let mut c = Connection {
            stream: None,
            host: addr,
            password,
            authenticated: false,
            max_retries: self.max_retries,
            retry_delay: self.retry_delay,
            exponential_backoff: self.exponential_backoff,
        };

        c.connect().await?;
        c.login().await?;
        Ok(c)
    }

    pub fn max_retries(mut self, retries: u32) -> Self {
        self.max_retries = retries;
        self
    }

    pub fn retry_delay(mut self, retry_delay: Duration) -> Self {
        self.retry_delay = retry_delay;
        self
    }

    pub fn exponential_backoff(mut self, exponential_backoff: bool) -> Self {
        self.exponential_backoff = exponential_backoff;
        self
    }
}

impl Connection {
    pub fn builder() -> Builder {
        Builder {
            max_retries: 3,
            retry_delay: Duration::from_millis(1000),
            exponential_backoff: false,
        }   
    }

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
        return Err(io::Error::new(io::ErrorKind::NotFound, "unable to resolve host"));   
    }

    // maybe check if the client is authenicated before allowing this
    async fn login(&mut self) -> io::Result<()> {
        self.authenticated = false;
        let aid = thread_rng().gen::<i32>();

        let pk = Packet {
            ptype: PacketType::Auth,
            id: aid,
            body: self.password.clone(),
        };

        let stream = self.stream.as_mut().ok_or(io::Error::new(io::ErrorKind::NotConnected, "Not connected"))?;

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
        return Err(io::Error::new(io::ErrorKind::InvalidData, "Recieved Invalid data from server"))
    }

    pub async fn run(&mut self, cmd: String) -> io::Result<Packet> {
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

        match stream.next().await {
            Some(Ok(x)) => Ok(x),
            Some(Err(e)) => Err(e),
            None => Err(Error::new(
                ErrorKind::ConnectionAborted,
                "Server ended the connection",
            )),
        }
    }
}
