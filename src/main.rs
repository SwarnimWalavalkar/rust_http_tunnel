mod dns;
mod codec;
use crate::dns::{DnsResolver, SimpleDnsResolver};
use crate::codec::{HttpCodec, TunnelResult};

use std::{env, net::SocketAddr};
use core::fmt::Debug;

use tokio::net::{TcpListener, TcpStream};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::io::AsyncWriteExt;

use tokio::signal;
use tokio::time::timeout;

use tokio_util::codec::Encoder;
use futures::StreamExt;

const PROXY_INITIAL_RESPONSE_SIZE: usize = 64;
const PROXY_CONNECT_TARGET_TIMEOUT: tokio::time::Duration = tokio::time::Duration::from_millis(200);

type AsyncResult<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

async fn tunnel_relay<R, W>(mut reader: R, mut writer: W, addr: SocketAddr) -> AsyncResult<()>
    where R: AsyncRead + Send + Unpin + 'static,
          W: AsyncWrite + Send + Unpin + 'static
{
    let mut codec = HttpCodec {};
    let mut response_buffer = bytes::BytesMut::with_capacity(PROXY_INITIAL_RESPONSE_SIZE);

    // connect to destination then write ok response then relay data in both direction
    match timeout(PROXY_CONNECT_TARGET_TIMEOUT,
                  TcpStream::connect(addr)).await {
        Ok(Ok(stream)) => {

            // write response to proxy
            codec.encode(TunnelResult::Ok, &mut response_buffer)?;
            writer.write_buf(&mut response_buffer).await?;

            stream.writable().await?;
            let (mut stream_reader, mut stream_writer) = stream.into_split();
            tokio::spawn(async move {
                // from proxy client to dest writer
                tokio::io::copy(&mut reader, &mut stream_writer).await
            });

            tokio::spawn(async move {
                // from dest reader to proxy writer
                tokio::io::copy(&mut stream_reader, &mut writer).await
            });
        }
        Ok(Err(e)) => {
            // connect error
            println!("Could not connect to {}: {}", addr, e);
            codec.encode(TunnelResult::Timeout, &mut response_buffer)?;
            writer.write_buf(&mut response_buffer).await?;
        }
        Err(e) => {
            // timeout
            println!("Timeout while trying to connect to {}: {}", addr, e);
            codec.encode(TunnelResult::BadRequest, &mut response_buffer)?;
            writer.write_buf(&mut response_buffer).await?;
        },
    }

    Ok(())
}

async fn tunnel_stream<R, W, D>(reader: R, writer: W, mut resolver: D) -> AsyncResult<()>
    where R: AsyncRead + Send + Unpin + Debug + 'static,
          W: AsyncWrite + Send + Unpin + 'static,
          D: DnsResolver {
    let codec = HttpCodec {};

    let mut fr = tokio_util::codec::FramedRead::new(reader, codec);

    if let Ok(url_) = fr.next().await.ok_or("Cannot read frame")? {
        let addr = resolver.resolve(&url_).await?;
        let reader = fr.into_inner(); // get back reader
        tokio::spawn(tunnel_relay(reader, writer, addr));
    }
    Ok(())
}

async fn tunnel() -> AsyncResult<()> {
  let arg: Vec<String> = env::args().skip(1).take(1).collect();
  let resolver = SimpleDnsResolver::new();

  let addr = match arg.len() {
      0 => panic!("Please provide a host:port like 127.0.0.1:7070"),
      1 => &arg[0],
      _ => panic!("Something went wrong"),
  };
    
  let listener = TcpListener::bind(&addr[..]).await?;
  println!("[Tcp] Proxy Listening on {}", addr);
  loop {
      let (socket, _addr) = listener.accept().await?;
      socket.writable().await?;
      let (reader, writer) = socket.into_split();
      let resolver_ = resolver.clone();

      tokio::spawn(async move {
          if let Err(e) = tunnel_stream(reader, writer, resolver_).await {
              println!("[Tcp] Tunnel stream error: {}", e);
          }
      });
  }
        
    
}

async fn app() -> AsyncResult<()> {
    println!("Starting http tunnel...");
    tokio::select! {
        tunnel_result = tunnel() => {
            if tunnel_result.is_err() {
                println!("Unable to start tunnel: {:?}", tunnel_result);
            }
        },
        _ = signal::ctrl_c() => { println!("\nReceived [Ctrl-C]..."); },
    };
    Ok(())
}
fn main() {

    // init the tokio async runtime - default is a multi threaded runtime
    let rt = tokio::runtime::Runtime::new().unwrap();
    // app() is our main entry point
    rt.block_on(app()).unwrap();

}