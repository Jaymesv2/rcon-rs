use async_trait::async_trait;
use futures::{FutureExt, SinkExt, StreamExt};
use log::*;
use std::{collections::HashMap, io, sync::Arc};
use tokio::{
    net::{TcpListener, TcpStream, ToSocketAddrs},
    sync::{Mutex, RwLock},
};
use tokio_util::codec::*;
pub use typemap::TypeMap;

use super::*;
use packet::*;

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
    pub fn new() -> Self {
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
    stream: Framed<TcpStream, PacketCodec>,
    authenticated: bool,
    execer: Arc<Mutex<T>>,
}

impl<T: RconImpl> ServerSession<T> {
    pub fn from_tcp_stream(stream: TcpStream, execer: T) -> ServerSession<T> {
        let stream = Framed::new(stream, PacketCodec::new_server());

        ServerSession {
            stream,
            execer: Arc::new(Mutex::new(execer)),
            authenticated: false,
        }
    }

    // returns when the session ends
    pub async fn start(&mut self) -> io::Result<()> {
        debug!("starting client loop");
        loop {
            let authenticated = self.authenticated;
            let msg = self.stream.next().await;
            debug!("recieved packet {:?}", msg);
            match msg {
                Some(Ok(s)) if s.ptype == PacketType::ExecCommand && authenticated => {
                    let mut lock = self.execer.lock().await;
                    let ret = T::process(&mut *lock, s.body).await;
                    let pk = Packet {
                        ptype: PacketType::ResponseValue,
                        id: s.id,
                        body: ret.unwrap(),
                    };
                    let _ = self.stream.send(pk).await;
                }
                Some(Ok(s)) if s.ptype == PacketType::Auth && !authenticated => {
                    let mut l = self.execer.lock().await;
                    if T::authenticate(&mut *l, s.body, s.id).await {
                        debug!("authenticated user");
                        self.authenticated = true;
                        let _ = self
                            .stream
                            .send(Packet {
                                id: s.id,
                                ptype: PacketType::ResponseValue,
                                body: String::new(),
                            })
                            .await;
                    } else {
                        debug!("failed to authenticate user");
                        let _ = self
                            .stream
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
