use bytes::{Buf, BufMut, Bytes, BytesMut};
use log::*;
use std::io::{Read, self};
use tokio_util::codec::*;

#[derive(PartialEq, Debug)]
pub enum PacketType {
    Auth,
    AuthResponse,
    ExecCommand,
    ResponseValue,
}

impl PacketType {
    pub fn from_i32(i: i32, response: bool) -> Result<PacketType, ()> {
        match i {
            0 => Ok(PacketType::ResponseValue),
            2 if !response => Ok(PacketType::AuthResponse),
            2 => Ok(PacketType::ExecCommand),
            3 => Ok(PacketType::Auth),
            _ => Err(()),
        }
    }
    pub fn bytes(&self) -> i32 {
        match self {
            PacketType::ResponseValue => 0,
            PacketType::AuthResponse => 2,
            PacketType::ExecCommand => 2,
            PacketType::Auth => 3,
        }
    }
}

#[derive(Debug)]
pub struct Packet {
    pub ptype: PacketType,
    pub id: i32,
    pub body: String,
}

impl Packet {
    pub fn from_bytes(b: Bytes, is_client: bool) -> Result<Packet, PacketProcessError> {
        let mut buf = b.clone();
        if buf.remaining() < 10 || buf.remaining() > 4096 {
            return Err(PacketProcessError::Length);
        }
        let msg_id = buf.get_i32_le();
        let ptype = PacketType::from_i32(buf.get_i32_le(), !is_client).map_err(|e| {
            error!("received bad packet from client {:?}", e);
            PacketProcessError::ParseError
        })?;

        let mut body = String::new(); //String::from_utf8(buf.take(buf.remaining() - 10)).unwrap();
        let rem = buf.remaining() - 2;
        buf.take(rem).reader().read_to_string(&mut body).unwrap();
        Ok(Packet {
            ptype,
            id: msg_id,
            body,
        })
    }

    pub fn bytes(&self) -> Bytes {
        let mut buf = BytesMut::new();
        self.write_bytes(&mut buf);
        buf.freeze()
    }

    pub fn len(&self) -> usize {
        &self.body.len() + 10
    }

    /// Writes the packets
    pub fn write_bytes(&self, buf: &mut BytesMut) -> usize {
        buf.put_i32_le(self.id);
        buf.put_i32_le(self.ptype.bytes());
        buf.put_slice(&self.body.clone().as_bytes());
        buf.put_slice(&[0x00, 0x00]);
        dbg!(buf);
        &self.body.len() + 10
    }
}

#[derive(Debug)]
pub enum PacketProcessError {
    Io(std::io::Error),
    Length,
    ParseError,
    StreamEnded,
}


pub struct PacketCodec {
    state: DecodeState,
    is_client: bool,
}

impl PacketCodec {
    pub fn new(is_client: bool)-> PacketCodec {
        PacketCodec {
            state: DecodeState::Head,
            is_client,
        }
    }

    pub fn new_client() -> PacketCodec {
        Self::new(true)
    }

    pub fn new_server() -> PacketCodec {
        Self::new(false)
    }
}

enum DecodeState {
    Head,
    Data(usize),
}

impl Encoder<Packet> for PacketCodec {
    type Error = io::Error;

    fn encode(&mut self, item: Packet, dst: &mut BytesMut) -> io::Result<()> {
        dst.put_i32_le(item.len() as i32);
        item.write_bytes(dst);
        Ok(())
    }
}

impl Decoder for PacketCodec {
    type Item = Packet;
    type Error = io::Error;

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
                        Packet::from_bytes(data, self.is_client).expect("failed to deserialize packet"),
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
