use bytes::{Buf, BufMut, Bytes, BytesMut};
use std::io::{self, Error, ErrorKind, Read};
use tokio_util::codec::*;

#[derive(PartialEq, Debug)]
pub enum PacketType {
    Auth,
    AuthResponse,
    ExecCommand,
    ResponseValue,
}

impl PacketType {
    pub fn from_i32(i: i32, response: bool) -> Option<PacketType> {
        match i {
            0 => Some(PacketType::ResponseValue),
            2 if !response => Some(PacketType::AuthResponse),
            2 => Some(PacketType::ExecCommand),
            3 => Some(PacketType::Auth),
            _ => None,
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
    pub fn from_bytes(mut b: Bytes, codec: CodecType) -> io::Result<Packet> {
        if b.remaining() < 10 || b.remaining() > 4096 {
            return Err(Error::new(
                ErrorKind::InvalidInput,
                "Message length was less than the minimum (10 bytes)",
            ));
        }
        let msg_id = b.get_i32_le();
        let ptype = PacketType::from_i32(b.get_i32_le(), codec == CodecType::Server).ok_or_else(|| Error::new(ErrorKind::InvalidInput, "Undefined message id"))?;

        let mut body = String::new();
        let rem = b.remaining() - 2;
        b.take(rem).reader().read_to_string(&mut body).unwrap();
        Ok(Packet {
            ptype,
            id: msg_id,
            body,
        })
    }

    pub fn encoded_len(&self) -> usize {
        &self.body.len() + 10
    }

    pub fn write_bytes(&self, buf: &mut BytesMut) -> usize {
        buf.put_i32_le(self.id);
        buf.put_i32_le(self.ptype.bytes());
        buf.put_slice(self.body.clone().as_bytes());
        buf.put_slice(&[0x00, 0x00]);
        &self.body.len() + 10
    }
}
// https://developer.valvesoftware.com/wiki/Source_RCON_Protocol#Packet_Size
// the rcon spec says that packets cannot be more than 4096 bytes
pub struct PacketCodec {
    state: DecodeState,
    ctype: CodecType,
    max_length: usize,
}

impl PacketCodec {
    /// Creates a new PacketCodec,
    /// WARNING: The [RCON spec](https://developer.valvesoftware.com/wiki/Source_RCON_Protocol#Packet_Size) sets a maximum packet size of 4096 bytes, raising it higher may cause issues with some clients.
    pub fn new(codec_type: CodecType, max_length: usize) -> PacketCodec {
        PacketCodec {
            state: DecodeState::Head,
            ctype: codec_type,
            max_length,
        }
    }
    #[cfg(feature = "client")]
    pub fn new_client() -> PacketCodec {
        Self::new(CodecType::Client, 4096)
    }

    #[cfg(feature = "server")]
    pub fn new_server() -> PacketCodec {
        Self::new(CodecType::Server, 4096)
    }
}
#[derive(PartialEq, Clone, Copy)]
pub enum CodecType {
    Client,
    Server
}

enum DecodeState {
    Head,
    Data(usize),
    Ignore(usize),
}

impl Encoder<Packet> for PacketCodec {
    type Error = io::Error;

    fn encode(&mut self, item: Packet, dst: &mut BytesMut) -> io::Result<()> {
        dst.put_i32_le(item.encoded_len() as i32);
        item.write_bytes(dst);
        Ok(())
    }
}

impl Decoder for PacketCodec {
    type Item = Packet;
    type Error = io::Error;

    fn decode(&mut self, src: &mut BytesMut) -> io::Result<Option<Self::Item>> {
        if src.is_empty() {
            return Ok(None);
        }

        let packet_len = match self.state {
            DecodeState::Head => {
                // ensures that src.get_i32_le() doesnt panic
                if src.len() <= 4 {
                    return Ok(None);
                };
                let packet_len = src.get_i32_le() as usize;
                if src.len() > self.max_length {
                    if src.len() >= packet_len {
                        let _ = src.split_to(packet_len);
                        return Ok(None);
                    } else {
                        let remaining = src.len();
                        src.clear();
                        self.state = DecodeState::Ignore(remaining);
                        return Ok(None);
                    }
                    //return Err(Error::new(ErrorKind::InvalidData, "client sent a packet which was larger than the maximum allowed packet length"));
                } else if src.len() < packet_len {
                    self.state = DecodeState::Data(packet_len);
                    return Ok(None);
                }
                packet_len
            }
            DecodeState::Data(packet_len) => {
                if packet_len < src.len() {
                    return Ok(None);
                }
                packet_len
            }
            DecodeState::Ignore(remaining) => {
                if !src.is_empty() {
                    if src.len() >= remaining {
                        let _ = src.split_to(remaining);
                        self.state = DecodeState::Head;
                    } else {
                        self.state = DecodeState::Ignore(remaining - src.len());
                        src.clear();
                    }
                }
                return Ok(None);
            }
        };

        let data = src.split_to(packet_len).freeze();
        Ok(Some(Packet::from_bytes(data, self.ctype)?))
    }
}
