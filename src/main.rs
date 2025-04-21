use std::error::Error;
use std::net::SocketAddr;
use std::net::{Ipv4Addr, SocketAddrV4};
use std::str::FromStr;

use clap::Parser;
use nix::sys::socket::SockaddrIn;
use nix::sys::socket::sockopt::{Mark, OriginalDst};
use nix::sys::socket::{getsockopt, setsockopt};
use tokio::io::copy_bidirectional;
use tokio::net::{TcpListener, TcpSocket};

#[derive(Parser, Debug)]
#[command(version, about)]
struct Args {
    #[arg(short, long)]
    listen: Listen,

    #[arg(short, long)]
    mark: Option<u32>,
}

#[derive(Clone, Debug)]
enum Listen {
    Addr(SocketAddrV4),
    Port(u16),
}

impl FromStr for Listen {
    type Err = Box<dyn Error + Send + Sync>;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(if s.contains(':') {
            Self::Addr(s.parse()?)
        } else {
            Self::Port(s.parse()?)
        })
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();
    let listen: SocketAddr = match args.listen {
        Listen::Addr(x) => x.into(),
        Listen::Port(x) => (Ipv4Addr::LOCALHOST, x).into(),
    };
    let mark = args.mark;

    let listener = TcpListener::bind(listen).await?;
    loop {
        let (mut tcpstream, addr) = listener.accept().await?;
        tokio::spawn(async move {
            let orig_dst = match getsockopt(&tcpstream, OriginalDst) {
                Ok(x) => SockaddrIn::from(x),
                Err(e) => {
                    println!("{addr} -> failed to get the original destination: {e}");
                    return;
                }
            };

            let upstream = async {
                let tcpsocket = TcpSocket::new_v4()?;
                if let Some(x) = mark {
                    setsockopt(&tcpsocket, Mark, &x)?;
                }
                tcpsocket
                    .connect((orig_dst.ip(), orig_dst.port()).into())
                    .await
            }
            .await;

            let done = match upstream {
                Ok(mut x) => {
                    println!("{addr} -> {orig_dst} connected");
                    copy_bidirectional(&mut tcpstream, &mut x).await
                }
                Err(e) => {
                    println!("{addr} -> {orig_dst} failed: {e}");
                    return;
                }
            };

            match done {
                Ok((tx, rx)) => {
                    println!("{addr} -> {orig_dst} done: {tx} byte sent, {rx} byte received")
                }
                Err(e) => println!("{addr} -> {orig_dst} error: {e}"),
            }
        });
    }
}
