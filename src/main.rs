use std::error::Error;
use std::net::{Ipv4Addr, SocketAddrV4};

use clap::Parser;
use nix::sys::socket::sockopt::{Mark, OriginalDst};
use nix::sys::socket::{getsockopt, setsockopt};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::{TcpListener, TcpSocket};

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(short, long)]
    port: u16,

    #[arg(short, long)]
    mark: Option<u32>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();
    let port = args.port;
    let mark = args.mark;

    let listener = TcpListener::bind((Ipv4Addr::UNSPECIFIED, port)).await?;
    loop {
        let (tcpstream, addr) = listener.accept().await?;
        tokio::spawn(async move {
            let orig_dst = getsockopt(&tcpstream, OriginalDst).map_err(error)?;
            let orig_dst = SocketAddrV4::new(
                Ipv4Addr::from_bits(u32::from_be(orig_dst.sin_addr.s_addr)),
                u16::from_be(orig_dst.sin_port),
            );

            let tcpsocket = TcpSocket::new_v4().map_err(error)?;
            if let Some(x) = mark {
                setsockopt(&tcpsocket, Mark, &x).map_err(error)?;
            }
            let upstream = tcpsocket.connect(orig_dst.into()).await.map_err(error)?;
            println!("{addr} -> {orig_dst} connected");
            link_stream(tcpstream, upstream).await.map_err(error)?;

            println!("{addr} -> {orig_dst} disconnected");
            Ok::<_, ()>(())
        });
    }
}

fn error<E: Error + 'static>(e: E) {
    println!("{e}");
}

async fn link_stream<A: AsyncRead + AsyncWrite, B: AsyncRead + AsyncWrite>(
    a: A,
    b: B,
) -> std::io::Result<()> {
    let (ar, aw) = &mut tokio::io::split(a);
    let (br, bw) = &mut tokio::io::split(b);

    let r = tokio::select! {
        r1 = tokio::io::copy(ar, bw) => {
            r1
        },
        r2 = tokio::io::copy(br, aw) => {
            r2
        }
    };

    r.map(drop)
}
