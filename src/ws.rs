//! WebSocket (RFC 6455) mínimo: handshake e quadros. Sem dependências externas.

use std::io::{self, Read, Write};

pub const TEXT: u8 = 0x1;
pub const BINARY: u8 = 0x2;
pub const CLOSE: u8 = 0x8;
pub const PING: u8 = 0x9;
pub const PONG: u8 = 0xA;

/// Teto de uma mensagem remontada vinda do cliente.
const MAX_MESSAGE: usize = 1 << 20;
const GUID: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";

pub fn sha1(data: &[u8]) -> [u8; 20] {
    let mut h: [u32; 5] = [
        0x6745_2301,
        0xEFCD_AB89,
        0x98BA_DCFE,
        0x1032_5476,
        0xC3D2_E1F0,
    ];
    let mut msg = data.to_vec();
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&((data.len() as u64) * 8).to_be_bytes());
    for chunk in msg.chunks(64) {
        let mut w = [0u32; 80];
        for (i, b) in chunk.chunks(4).enumerate() {
            w[i] = u32::from_be_bytes([b[0], b[1], b[2], b[3]]);
        }
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }
        let [mut a, mut b, mut c, mut d, mut e] = h;
        for (i, wi) in w.iter().enumerate() {
            let (f, k) = match i {
                0..=19 => ((b & c) | (!b & d), 0x5A82_7999),
                20..=39 => (b ^ c ^ d, 0x6ED9_EBA1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1B_BCDC),
                _ => (b ^ c ^ d, 0xCA62_C1D6u32),
            };
            let t = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(k)
                .wrapping_add(*wi);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = t;
        }
        for (hv, v) in h.iter_mut().zip([a, b, c, d, e]) {
            *hv = hv.wrapping_add(v);
        }
    }
    let mut out = [0u8; 20];
    for (i, v) in h.iter().enumerate() {
        out[i * 4..i * 4 + 4].copy_from_slice(&v.to_be_bytes());
    }
    out
}

pub fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let n = (c[0] as u32) << 16
            | (*c.get(1).unwrap_or(&0) as u32) << 8
            | *c.get(2).unwrap_or(&0) as u32;
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if c.len() > 1 {
            T[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if c.len() > 2 {
            T[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

/// Valor de `Sec-WebSocket-Accept` para a chave do cliente.
pub fn accept_key(key: &str) -> String {
    base64(&sha1(format!("{key}{GUID}").as_bytes()))
}

pub fn handshake_response(key: &str) -> String {
    format!(
        "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {}\r\n\r\n",
        accept_key(key)
    )
}

fn invalid(msg: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, msg.to_string())
}

/// Lê a próxima mensagem do cliente: `(opcode, carga)`. Remonta fragmentos de dados; quadros de
/// controle (ping, pong, close) são devolvidos assim que chegam. Quadros sem máscara são recusados.
pub fn read_message(r: &mut impl Read) -> io::Result<(u8, Vec<u8>)> {
    let (mut op, mut data) = (0u8, Vec::new());
    loop {
        let mut h = [0u8; 2];
        r.read_exact(&mut h)?;
        let (fin, opcode, masked) = (h[0] & 0x80 != 0, h[0] & 0x0F, h[1] & 0x80 != 0);
        let len = match h[1] & 0x7F {
            126 => {
                let mut b = [0u8; 2];
                r.read_exact(&mut b)?;
                u16::from_be_bytes(b) as u64
            }
            127 => {
                let mut b = [0u8; 8];
                r.read_exact(&mut b)?;
                u64::from_be_bytes(b)
            }
            n => n as u64,
        };
        if !masked {
            return Err(invalid("quadro do cliente sem máscara"));
        }
        if len > MAX_MESSAGE as u64 || data.len() + len as usize > MAX_MESSAGE {
            return Err(invalid("mensagem grande demais"));
        }
        let mut key = [0u8; 4];
        r.read_exact(&mut key)?;
        let mut p = vec![0u8; len as usize];
        r.read_exact(&mut p)?;
        for (i, b) in p.iter_mut().enumerate() {
            *b ^= key[i % 4];
        }
        if opcode >= 8 {
            return Ok((opcode, p));
        }
        if opcode != 0 {
            op = opcode;
            data.clear();
        }
        data.extend_from_slice(&p);
        if fin {
            return Ok((op, data));
        }
    }
}

/// Envia um quadro do servidor (sem máscara).
pub fn write_frame(w: &mut impl Write, opcode: u8, payload: &[u8]) -> io::Result<()> {
    let mut f = vec![0x80 | opcode];
    match payload.len() {
        n if n < 126 => f.push(n as u8),
        n if n <= 0xFFFF => {
            f.push(126);
            f.extend_from_slice(&(n as u16).to_be_bytes());
        }
        n => {
            f.push(127);
            f.extend_from_slice(&(n as u64).to_be_bytes());
        }
    }
    f.extend_from_slice(payload);
    w.write_all(&f)?;
    w.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(b: &[u8]) -> String {
        b.iter().map(|x| format!("{x:02x}")).collect()
    }

    #[test]
    fn sha1_known_vectors() {
        assert_eq!(
            hex(&sha1(b"abc")),
            "a9993e364706816aba3e25717850c26c9cd0d89d"
        );
        assert_eq!(hex(&sha1(b"")), "da39a3ee5e6b4b0d3255bfef95601890afd80709");
        assert_eq!(
            hex(&sha1(
                b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"
            )),
            "84983e441c3bd26ebaae4aa1f95129e5e54670f1"
        );
    }

    #[test]
    fn base64_known_vectors() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn accept_key_matches_rfc_6455_example() {
        assert_eq!(
            accept_key("dGhlIHNhbXBsZSBub25jZQ=="),
            "s3pPLMBiTxaQ9kYGzzhZRbK+xOo="
        );
    }

    #[test]
    fn reads_the_masked_hello_from_the_rfc() {
        let frame = [
            0x81, 0x85, 0x37, 0xfa, 0x21, 0x3d, 0x7f, 0x9f, 0x4d, 0x51, 0x58,
        ];
        let (op, p) = read_message(&mut &frame[..]).unwrap();
        assert_eq!((op, p.as_slice()), (TEXT, &b"Hello"[..]));
    }

    #[test]
    fn rejects_unmasked_client_frames() {
        let frame = [0x81, 0x05, b'H', b'e', b'l', b'l', b'o'];
        assert!(read_message(&mut &frame[..]).is_err());
    }

    #[test]
    fn reassembles_fragments_and_returns_control_frames() {
        // "Hel" (sem FIN) + "lo" (continuação, FIN), ambos com máscara zero
        let frame = [
            0x01, 0x83, 0, 0, 0, 0, b'H', b'e', b'l', 0x80, 0x82, 0, 0, 0, 0, b'l', b'o',
        ];
        assert_eq!(
            read_message(&mut &frame[..]).unwrap(),
            (TEXT, b"Hello".to_vec())
        );
        let ping = [0x89, 0x80, 1, 2, 3, 4];
        assert_eq!(read_message(&mut &ping[..]).unwrap(), (PING, vec![]));
    }

    #[test]
    fn writes_short_and_long_server_frames() {
        let mut out = Vec::new();
        write_frame(&mut out, TEXT, b"Hello").unwrap();
        assert_eq!(out, [0x81, 0x05, b'H', b'e', b'l', b'l', b'o']);
        let mut big = Vec::new();
        write_frame(&mut big, BINARY, &[7u8; 300]).unwrap();
        assert_eq!(&big[..4], &[0x82, 126, 0x01, 0x2C]);
        assert_eq!(big.len(), 4 + 300);
    }
}
