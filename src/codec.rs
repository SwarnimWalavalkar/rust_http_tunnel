use tokio_util::codec::{Decoder, Encoder};
use bytes::BytesMut;

use std::fmt::Write;


#[derive(Debug)]
pub struct HttpCodec {
}

const MAX_HTTP_CONNECT_SIZE: usize = 1024; // enough for "CONNECT ..."
const HTTP_CONNECT_START: &[u8] = b"CONNECT ";
const HTTP_CONNECT_SLICE_START: usize = HTTP_CONNECT_START.len();

#[derive(Debug, thiserror::Error)]
pub enum DecodeError {
    #[error("i/o error: {0}")]
    IO(#[from] std::io::Error),
    #[error("utf8 error: {0}")]
    UTF8(#[from] std::string::FromUtf8Error)
}

fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|window| window == needle)
}

impl Decoder for HttpCodec {

    type Item = String;
    type Error = DecodeError;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {

        if !src.ends_with(b"\r\n") {
            return Ok(None); // not enough data
        }

        if src.len() >= MAX_HTTP_CONNECT_SIZE {
            return Err(DecodeError::IO(std::io::Error::new(std::io::ErrorKind::InvalidData,
                                           format!("HTTP frame too large: {}", src.len()))));
        }

        if !src.starts_with(HTTP_CONNECT_START) {
            return Err(DecodeError::IO(std::io::Error::new(std::io::ErrorKind::InvalidData,
                                           format!("Invalid HTTP request"))));
        }

        let http_connect_end_index = find_subsequence(src, b" HTTP/1.1\r\n");

        if http_connect_end_index.is_none() {
            return Err(DecodeError::IO(std::io::Error::new(std::io::ErrorKind::InvalidData,
                                           format!("Invalid HTTP request"))));
        }

        // unwrap is safe here
        let url_ : &[u8] = &src[HTTP_CONNECT_SLICE_START..http_connect_end_index.unwrap()];
        let url: String = String::from_utf8(url_.to_vec())?;
        Ok(Some(url))
    }

}

#[repr(u32)]
pub enum TunnelResult {
    Ok, // 200
    BadRequest, // 400
    // Forbidden, // 403
    Timeout, // 408
    // ServerError, // 500
}

impl Encoder<TunnelResult> for HttpCodec {

    type Error = std::io::Error;

    fn encode(&mut self, tunnel_result: TunnelResult, dst: &mut BytesMut) -> Result<(), Self::Error> {

        let (code, message): (u32, &str) = match tunnel_result {
            TunnelResult::Ok => (200, "OK"),
            TunnelResult::BadRequest => (400, "BAD_REQUEST"),
            TunnelResult::Timeout => (408, "Timeout"),
            // TunnelResult::Forbidden => (403, "Forbidden"),
            // TunnelResult::ServerError => (500, "SERVER_ERROR"),
        };

        dst.write_fmt(format_args!("HTTP/1.1 {} {}\r\n\r\n", code, message)).map_err(
            |_| std::io::Error::from(std::io::ErrorKind::Other)
        )
    }
}