use tokio::net::{tcp, TcpStream, ToSocketAddrs};
use tokio_util::codec::*;
use log::*;
use rand::{thread_rng, Rng};
use std::io;
use futures::{SinkExt, StreamExt};

use super::packet::*;

pub struct Client {
    stream: Framed<TcpStream, PacketCodec>,
    authenticated: bool,
}

impl Client {
    pub async fn new<S: ToSocketAddrs>(addr: S) -> io::Result<Client> {
        let stream = TcpStream::connect(addr).await?;

        let stream = Framed::new(stream, PacketCodec::new_client());

        Ok(Client {
            stream,
            authenticated: false,
        })
    }

    pub async fn login(&mut self, password: String) -> io::Result<bool> {
        let aid = thread_rng().gen::<i32>();
        let pk = Packet {
            ptype: PacketType::Auth,
            id: aid,
            body: password,
        };

        self.write_packet(pk).await?;

        loop {
            match self.stream.next().await {
                Some(Ok(p)) if p.ptype == PacketType::AuthResponse => {
                    self.authenticated = true;
                    return Ok(p.id == aid);
                }
                Some(Ok(_)) => {
                    debug!("client recieved non auth response when reading for auth response");
                    continue;
                }
                Some(Err(e)) => {
                    debug!("read error from buffer: {:?}", e);
                    continue;
                } // fix this
                None => return Err(io::Error::new(io::ErrorKind::ConnectionAborted, "")),
            }
        }
    }

    pub async fn run_command(&mut self, cmd: String) -> Result<Packet, PacketProcessError> {
        let pk = Packet {
            ptype: PacketType::ExecCommand,
            id: thread_rng().gen::<i32>(),
            body: cmd,
        };

        self.write_packet(pk)
            .await
            .map_err(|e| PacketProcessError::Io(e))?;

        match self.stream.next().await {
            Some(Ok(x)) => Ok(x),
            Some(Err(e)) => Err(PacketProcessError::Io(e)),
            None => Err(PacketProcessError::StreamEnded),
        }
    }

    pub async fn run(&mut self, cmd: String) -> Result<String, PacketProcessError> {
        Ok(self.run_command(cmd).await?.body)
    }

    pub async fn write_packet(&mut self, pack: Packet) -> io::Result<()> {
        self.stream.send(pack).await?;
        Ok(())
    }
}