use tokio_util::codec::*;
use log::*;
use std::{collections::HashMap, io, sync::Arc};
use bytes::{Buf, BufMut, BytesMut};
use futures::{FutureExt, SinkExt, StreamExt};
use typemap::TypeMap;
use tokio::{
    net::{tcp, TcpListener, TcpStream, ToSocketAddrs},
    sync::{Mutex, RwLock},
};

use super::*;
use packet::*;

/*
TODO:
  new abstraction for handling command filtering
  client authentication checking
*/

use async_trait::async_trait;

#[async_trait]
pub trait RconImpl {
    fn new(state: Arc<RwLock<TypeMap>>) -> Self;
    async fn authenticate(&mut self, password: String, pid: i32) -> bool; // change this return Result<bool, anyhow::Error>
    async fn process(&mut self, cmd: String) -> Result<String, anyhow::Error>;
}

pub struct RconServer<T: RconImpl> {
    state: Arc<RwLock<TypeMap>>,
    sessions: HashMap<i32, ServerSession<T>>, // change to vec with all sessions in it
}

impl<T: RconImpl + std::marker::Send + 'static> RconServer<T> {
    pub fn new() -> RconServer<T> {
        RconServer {
            state: Arc::new(RwLock::new(TypeMap::new())),
            sessions: HashMap::new(),
        }
    }

    pub async fn run<S: ToSocketAddrs>(&mut self, addr: S) {
        let listener = TcpListener::bind(addr).await.expect("failed to bind");

        loop {
            let (socket, addr) = match listener.accept().await {
                Ok(s) => s,
                Err(e) => {
                    warn!(
                        "an error occured while accepting a tcp connection, ignoring, {:?}",
                        e
                    );
                    continue;
                }
            };
            debug!("A tcp socket was accepted from {:?}", addr);

            let implimentor = T::new(Arc::clone(&self.state));

            let mut serv = ServerSession::from_tcp_stream(socket, implimentor);

            let _h = tokio::spawn(async move {
                //serv.start().await;
                let _ = serv
                    .start()
                    .map(|x| async {
                        debug!("completed thread with value {:?}", &x);
                        x
                    })
                    .await;
            });
        }
    }
}

pub struct ServerSession<T: RconImpl> {

    writer: FramedWrite<tcp::OwnedWriteHalf, PacketCodec>,
    reader: FramedRead<tcp::OwnedReadHalf, PacketCodec>,
    authenticated: bool,
    execer: Arc<Mutex<T>>,
}

impl<T: RconImpl> ServerSession<T> {
    pub fn from_tcp_stream(stream: TcpStream, execer: T) -> ServerSession<T> {
        let (raw_reader, raw_writer) = stream.into_split();

        let reader = FramedRead::new(raw_reader, PacketCodec::new());

        let writer = FramedWrite::new(raw_writer, PacketCodec::new());

        ServerSession {
            reader,
            writer,
            execer: Arc::new(Mutex::new(execer)),
            authenticated: false,
        }
    }

    // returns when the session ends
    pub async fn start(&mut self) -> io::Result<()> {
        debug!("starting client loop");
        loop {
            let authenticated = self.authenticated;
            let m = self.reader.next().await;
            debug!("recieved packet {:?}", m);
            match m {
                Some(Ok(s)) if s.ptype == PacketType::ExecCommand && authenticated => {
                    let mut l = self.execer.lock().await;
                    let r = T::process(&mut *l, s.body).await;
                    let p = Packet {
                        ptype: PacketType::ResponseValue,
                        id: s.id,
                        body: r.unwrap(),
                    };
                    let _ = self.writer.send(p).await;
                }
                Some(Ok(s)) if s.ptype == PacketType::Auth && !authenticated => {
                    let mut l = self.execer.lock().await;
                    if T::authenticate(&mut *l, s.body, s.id).await {
                        debug!("authenticated user");
                        self.authenticated = true;
                        let _ = self
                            .writer
                            .send(Packet {
                                id: s.id,
                                ptype: PacketType::ResponseValue,
                                body: String::new(),
                            })
                            .await;
                    } else {
                        debug!("failed to authenticate user");
                        let _ = self
                            .writer
                            .send(Packet {
                                id: -1,
                                ptype: PacketType::ResponseValue,
                                body: String::new(),
                            })
                            .await;
                    }
                }
                Some(Ok(s)) if s.ptype == PacketType::ExecCommand && !authenticated => {
                    warn!("client sending ExecCommand packets without authenticating");
                }
                Some(Ok(s)) => {
                    warn!("recieved other packet type: {:?}", s);
                }
                Some(Err(e)) => {
                    error!("{:?}", e);
                }
                None => {
                    debug!("finished thread");
                    return Ok(());
                }
            };
        }
    }
}

struct PacketCodec {
    state: DecodeState,
}

impl PacketCodec {
    pub fn new() -> PacketCodec {
        PacketCodec {
            state: DecodeState::Head,
        }
    }
}

enum DecodeState {
    Head,
    Data(usize),
}

impl Encoder<Packet> for PacketCodec {
    type Error = anyhow::Error;

    fn encode(&mut self, item: Packet, dst: &mut BytesMut) -> Result<(), Self::Error> {
        dst.put_i32_le(item.len() as i32);
        item.write_bytes(dst);
        Ok(())
    }
}

impl Decoder for PacketCodec {
    type Item = Packet;
    type Error = std::io::Error;

    fn decode(&mut self, src: &mut BytesMut) -> io::Result<Option<Self::Item>> {
        match self.state {
            DecodeState::Head => {
                let slen = src.len();
                if slen <= 4 {
                    return Ok(None);
                };
                let len = src.get_i32_le() as usize;
                if src.len() <= len {
                    let data = src.split_to(len).freeze();
                    return Ok(Some(
                        Packet::from_bytes(data, false).expect("failed to deserialize packet"),
                    ));
                } else {
                    panic!("too lazy to impliment multi packet processing");
                }
            }
            DecodeState::Data(_s) => {
                panic!("too lazy to impliment multi packet processing");
            }
        }
    }
}
