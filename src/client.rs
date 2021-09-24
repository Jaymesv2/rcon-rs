use tokio::net::{tcp, TcpStream, ToSocketAddrs};
use tokio_util::codec::{length_delimited::*, *};
use bytes::BytesMut;
use log::*;
use rand::{thread_rng, Rng};
use std::io;
use futures::{stream::Map, SinkExt, StreamExt};

use super::packet::*;

pub struct Client {
    reader: Map<
        FramedRead<tcp::OwnedReadHalf, LengthDelimitedCodec>,
        fn(Result<BytesMut, std::io::Error>) -> Result<Packet, PacketProcessError>,
    >,
    writer: FramedWrite<tcp::OwnedWriteHalf, LengthDelimitedCodec>,
    authenticated: bool,
}

impl Client {
    pub async fn new<S: ToSocketAddrs>(addr: S) -> io::Result<Client> {
        let stream = TcpStream::connect(addr).await?;

        let (raw_reader, raw_writer) = stream.into_split();

        let reader = LengthDelimitedCodec::builder()
            .length_field_offset(0)
            .length_field_length(4)
            .length_adjustment(0)
            //.num_skip(0)
            .little_endian()
            .new_read(raw_reader)
            .map(
                packet_from_stream_client
                    as fn(Result<BytesMut, std::io::Error>) -> Result<Packet, PacketProcessError>,
            );
        //.map(packet_from_stream_client);

        let writer = LengthDelimitedCodec::builder()
            .length_field_offset(0)
            .length_field_length(4)
            .length_adjustment(0)
            .little_endian()
            .new_write(raw_writer);

        Ok(Client {
            reader,
            writer,
            authenticated: false,
        })
    }

    pub async fn login(&mut self, password: String) -> io::Result<bool> {
        let pk = Packet {
            ptype: PacketType::Auth,
            id: thread_rng().gen::<i32>(),
            body: password,
        };

        self.write_packet(&pk).await?;

        loop {
            match self.reader.next().await {
                Some(Ok(p)) if p.ptype == PacketType::AuthResponse => {
                    self.authenticated = true;
                    return Ok(p.id == pk.id);
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
        /*if self.authenticated {
            return Err()
        }*/

        let pk = Packet {
            ptype: PacketType::ExecCommand,
            id: thread_rng().gen::<i32>(),
            body: cmd,
        };

        self.write_packet(&pk)
            .await
            .map_err(|e| PacketProcessError::Io(e))?;

        match self.reader.next().await {
            Some(x) => x,
            None => Err(PacketProcessError::StreamEnded),
        }
    }

    pub async fn run(&mut self, cmd: String) -> Result<String, PacketProcessError> {
        Ok(self.run_command(cmd).await?.body)
    }

    pub async fn write_packet(&mut self, pack: &Packet) -> io::Result<()> {
        self.writer.send(pack.bytes()).await?;
        Ok(())
    }
}

fn packet_from_stream_client(s: io::Result<BytesMut>) -> Result<Packet, PacketProcessError> {
    match s.map(|b| Packet::from_bytes(b.freeze(), true)) {
        Ok(Ok(s)) => Ok(s),
        Ok(Err(e)) => Err(e),
        Err(e) => Err(PacketProcessError::Io(e)),
    }
}
