use anyhow::{Context, Result, anyhow};
use slog::{Logger, info, warn};
use std::{
    borrow::{Borrow, BorrowMut},
    collections::HashMap,
    net::{self, IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, ToSocketAddrs},
    ops::Range,
    random::{Rng, SystemRng},
    result::Result as StdResult,
};

use crate::comm::{self, Port};

// STUN message header — RFC 5389 §6
//
// Fixed 20 bytes, everything big-endian (network order):
//
//   0                   1                   2                   3
//   0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
//  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//  |0 0|     STUN Message Type     |         Message Length        |
//  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//  |                         Magic Cookie (0x2112A442)             |
//  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//  |                                                               |
//  |                     Transaction ID (12 bytes)                 |
//  |                                                               |
//  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//  NOTE: Array is in network order = big-endian encoding for integers.
//
#[derive(Debug, Copy, Clone)]
struct StunHeaderImpl<T>(T);
type StunHeader = StunHeaderImpl<[u8; HEADER_LEN]>;
type StunHeaderRef<'a> = StunHeaderImpl<&'a [u8; HEADER_LEN]>;

pub const PUBLIC_STUN_SERVER: (&str, u16) = ("stun.l.google.com", 19302);

const HEADER_LEN: usize = 20;
const HEADER_TYPE: Range<usize> = 0..2; // Big-endian u16.
const HEADER_LENGTH: Range<usize> = 2..4; // Big-endian u16.
const HEADER_COOKIE: Range<usize> = 4..8; // Big-endian u32.
const HEADER_TXID: Range<usize> = 8..20;

const MAGIC_COOKIE: u32 = 0x2112_A442; // In Big-endian u32.

// The message type in the header is interleaved with class and method:
// u16: `0 0 M11 M10 M9 M8 M7 C1 M6 M5 M4 C0 M3 M2 M1 M0`:
//
//   - class  0x00  = request,
//            0x01  = indication,
//            0x10  = success response,
//            0x11  = error response
//   - method 0x001 = Binding (the only one we need)
//
//  which gives:
#[repr(u16)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[allow(clippy::enum_variant_names)]
enum MsgType {
    BindingRequest = 0x0001,
    BindingResponse = 0x0101,
    BindingResponseError = 0x0111,
}

impl TryFrom<u16> for MsgType {
    type Error = u16;
    fn try_from(v: u16) -> StdResult<Self, Self::Error> {
        match v {
            x if x == MsgType::BindingRequest as u16 => Ok(MsgType::BindingRequest),
            x if x == MsgType::BindingResponse as u16 => Ok(MsgType::BindingResponse),
            x if x == MsgType::BindingResponseError as u16 => Ok(MsgType::BindingResponseError),
            _ => Err(v),
        }
    }
}

//  https://datatracker.ietf.org/doc/html/rfc5389#section-15
//  After the STUN header are zero or more attributes.  Each attribute
//  MUST be Type-Length-Value encoded, with a 16-bit type,
//  16-bit length, and value.
//  Each STUN attribute MUST end on a 32-bit boundary.  As mentioned
//  above, all fields in an attribute are transmitted most significant
//  bit first.
//
//   0                   1                   2                   3
//   0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
//  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//  |         Type                  |            Length             |
//  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
//  |                         Value (variable)                ....
//  +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
#[repr(u16)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
enum AttributeType {
    XorMappedAddress = 0x0020,
}

impl TryFrom<u16> for AttributeType {
    type Error = u16;
    fn try_from(v: u16) -> StdResult<Self, Self::Error> {
        match v {
            x if x == AttributeType::XorMappedAddress as u16 => Ok(AttributeType::XorMappedAddress),
            _ => Err(v),
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
enum Attribute {
    XorMappedAddress(XorMappedAddress),
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
struct XorMappedAddress {
    port: comm::Port,
    address: IpAddr,
}

impl<T: BorrowMut<[u8; HEADER_LEN]>> StunHeaderImpl<T> {
    fn set_message_type(&mut self, t: MsgType) -> &mut Self {
        let b: &mut [u8; HEADER_LEN] = self.0.borrow_mut();
        b[HEADER_TYPE].copy_from_slice(&(t as u16).to_be_bytes());
        self
    }

    fn set_length(&mut self, l: u16) -> &mut Self {
        let b: &mut [u8; HEADER_LEN] = self.0.borrow_mut();
        b[HEADER_LENGTH].copy_from_slice(&l.to_be_bytes());
        self
    }

    fn set_magic_cookie(&mut self) -> &mut Self {
        // The cookie is sent in network byte order.
        let b: &mut [u8; HEADER_LEN] = self.0.borrow_mut();
        b[HEADER_COOKIE].copy_from_slice(&MAGIC_COOKIE.to_be_bytes());
        self
    }

    fn set_transaction_id(&mut self) -> &mut Self {
        let b: &mut [u8; HEADER_LEN] = self.0.borrow_mut();
        let mut rnd = SystemRng;
        let mut id = [0u8; 12];
        rnd.fill_bytes(&mut id);
        b[HEADER_TXID].copy_from_slice(&id);
        self
    }
}

impl<T: Borrow<[u8; HEADER_LEN]>> StunHeaderImpl<T> {
    fn message_type(&self) -> Result<MsgType, u16> {
        let b: &[u8; HEADER_LEN] = self.0.borrow();
        let m = u16::from_be_bytes(
            b[HEADER_TYPE]
                .try_into()
                .expect("cannot fail: msg type must be 2 bytes"),
        );

        m.try_into()
    }

    #[allow(dead_code)]
    fn length(&self) -> u16 {
        let b: &[u8; HEADER_LEN] = self.0.borrow();
        u16::from_be_bytes(
            b[HEADER_LENGTH]
                .try_into()
                .expect("cannot fail: length type must be 2 bytes"),
        )
    }

    fn transaction_id(&self) -> &[u8; 12] {
        let b: &[u8; HEADER_LEN] = self.0.borrow();
        b[HEADER_TXID]
            .first_chunk::<12>()
            .expect("cannot fail: transaction id must be 12 bytes")
    }
}

impl StunHeader {
    fn new() -> StunHeader {
        let mut s = StunHeaderImpl([0; HEADER_LEN]);
        s.set_magic_cookie();
        s
    }

    pub fn new_binding_request() -> StunHeader {
        *StunHeader::new()
            .set_length(0)
            .set_transaction_id()
            .set_message_type(MsgType::BindingRequest)
    }
}

impl StunHeaderRef<'_> {
    fn new(d: &[u8; HEADER_LEN]) -> Result<StunHeaderRef<'_>> {
        let s = StunHeaderImpl(d);
        if !s.valid() {
            return Err(anyhow!("invalid stun header: magic cookie not found"));
        }

        Ok(s)
    }

    fn valid(self) -> bool {
        u32::from_be_bytes(
            self.0[HEADER_COOKIE]
                .try_into()
                .expect("cannot fail: msg type must be 2 bytes"),
        ) == MAGIC_COOKIE
    }
}

/// Sends a STUN message to a public stun server to discover our IP
/// address.
///
/// # Panics
/// # Errors
pub fn send_stun_binding_request(
    log: &Logger,
    ipv6: bool,
    socket: &net::UdpSocket,
    server: (&str, u16),
) -> Result<(IpAddr, Port)> {
    info!(log, "Resolve address to public server {server:?}.");
    let server: SocketAddr = server
        .to_socket_addrs()?
        .find(if ipv6 {
            SocketAddr::is_ipv6
        } else {
            SocketAddr::is_ipv4
        })
        .ok_or_else(|| anyhow!("no IPv4 address for stun.l.google.com"))?;

    let b = StunHeader::new_binding_request();

    info!(log, "Sending stun header to {server}:\n   {:02X?}", b.0);
    if let Err(e) = socket.send_to(&b.0, server) {
        warn!(log, "could not send: {e}");
        return Err(e).context("failed to send STUN binding response");
    }

    let mut buf = vec![0; 1500];
    let mut h: StunHeaderRef;
    let mut msg: &[u8];

    for tries in 0..10 {
        info!(log, "Receiving STUN response: {tries}/10.");

        match socket.recv_from(&mut buf) {
            Ok((n, addr)) if addr == server && n >= HEADER_LEN => {
                info!(log, "received: {:02X?}", &buf[0..n]);

                h = match StunHeaderRef::new(buf.first_chunk::<20>().unwrap()) {
                    Err(e) => {
                        warn!(log, "No STUN message received: {e}");
                        continue;
                    }
                    Ok(s) => s,
                };
                msg = &buf[HEADER_LEN..n];
            }
            Err(e) => {
                warn!(log, "- Receive failed: {e}.");
                continue;
            }
            _ => {
                warn!(log, "Received from other source.");
                continue;
            }
        }

        match h.message_type() {
            Ok(t) => match t {
                MsgType::BindingResponse => {
                    info!(log, "Response received");
                }
                MsgType::BindingResponseError => {
                    warn!(log, "Response error.");
                }
                MsgType::BindingRequest => {
                    warn!(log, "Should not receive a request.");
                }
            },
            Err(v) => {
                warn!(log, "Message type is not supported '{v}'");
                continue;
            }
        }

        if b.transaction_id() != h.transaction_id() {
            warn!(log, "Transaction ID is not the same, skip.");
            continue;
        }

        info!(log, "Header: {h:02X?}");
        info!(log, "Message: {msg:02X?}");

        let attrs = parse_attributes(log, h, msg);
        info!(log, "Attributes: {attrs:?}");

        let Some(addrs) = attrs.get(&AttributeType::XorMappedAddress) else {
            return Err(anyhow!("could not get address from STUN message"));
        };
        let Attribute::XorMappedAddress(a) = addrs[0];

        return Ok((a.address, a.port));
    }

    Err(anyhow!("failed STUN request/response"))
}

fn parse_attributes(
    log: &'_ Logger,
    header: StunHeaderRef,
    d: &'_ [u8],
) -> HashMap<AttributeType, Vec<Attribute>> {
    let mut attrs = HashMap::new();

    if !d.len().is_multiple_of(4) {
        warn!(log, "Attributes are not a multiple of 4 bytes.");

        return attrs;
    }

    let mut i = 0;

    while i + 4 <= d.len() {
        let ty = u16::from_be_bytes(*d[i..i + 2].first_chunk::<2>().unwrap());
        i += 2;
        let len = u16::from_be_bytes(*d[i..i + 2].first_chunk::<2>().unwrap()) as usize;
        i += 2;

        let jump_to_next_attr = |i: usize| i.next_multiple_of(4);

        let Some(val) = &d.get(i..i + len) else {
            warn!(
                log,
                "Length in attribute is corrupt: {i} + {len} >= {}.",
                d.len()
            );
            return attrs;
        };
        i += len;

        let Ok(ty) = AttributeType::try_from(ty) else {
            warn!(log, "Attribute '{ty}' not known.");
            i = jump_to_next_attr(i);
            continue;
        };

        warn!(log, "Attribute '{ty:?}' with length '{}'.", val.len());

        let attr: Attribute = match ty {
            AttributeType::XorMappedAddress => {
                info!(log, "Parsing XOR mapped address.");
                let Some(add) = parse_xor_mapped_address(header, val) else {
                    warn!(log, "Could not parse XOR mapped address.");
                    continue;
                };
                Attribute::XorMappedAddress(add)
            }
        };

        match attrs.get_mut(&ty) {
            Some(v) => {
                v.push(attr);
            }
            None => {
                attrs.insert(ty, vec![attr]);
            }
        }

        i = jump_to_next_attr(i);
    }

    attrs
}

fn parse_xor_mapped_address(header: StunHeaderRef, val: &[u8]) -> Option<XorMappedAddress> {
    let mut port: comm::Port = u16::from_be_bytes(*val[2..].first_chunk::<2>().unwrap());
    port ^= (MAGIC_COOKIE >> 16) as u16;

    let address: IpAddr = match val[1] {
        0x01 => {
            let mut address: u32 = u32::from_be_bytes(*val[4..8].first_chunk::<4>().unwrap());
            address ^= MAGIC_COOKIE;
            IpAddr::V4(Ipv4Addr::from_bits(address))
        }
        0x02 => {
            let mut address: u128 = u128::from_be_bytes(*val[4..20].first_chunk::<16>().unwrap());

            let mut key = [0u8; 16];
            key[..4].copy_from_slice(&MAGIC_COOKIE.to_be_bytes());
            key[4..].copy_from_slice(header.transaction_id());
            let key = u128::from_be_bytes(key);
            address ^= key;

            IpAddr::V6(Ipv6Addr::from_bits(address))
        }
        _ => return None,
    };

    Some(XorMappedAddress { port, address })
}

#[allow(unused_imports)]
mod test {
    use crate::stun::*;

    #[test]
    fn test_header() {
        let mut s = StunHeader::new();
        s.set_message_type(MsgType::BindingRequest).set_length(10);

        assert_eq!(s.length(), 10);
        assert_eq!(s.message_type().ok(), Some(MsgType::BindingRequest));

        let s = StunHeader::new();
        assert_eq!(s.length(), 0);
        assert_eq!(s.message_type().ok(), None);
    }
}
