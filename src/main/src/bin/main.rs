use anyhow::{Context, Result};
use clap::Parser;
use common::{
    comm::{self, parse_address},
    stun::{PUBLIC_STUN_SERVER, send_stun_binding_request},
};
use serde::{Deserialize, Serialize};
use slog::{Drain, Logger, info, o, warn};
use std::{
    net::{self, Ipv4Addr, Ipv6Addr, SocketAddr},
    thread::sleep,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    #[clap(
        short,
        long,
        default_value = "a",
        help = "Which side this client is on."
    )]
    side: comm::Side,

    #[clap(
        long,
        default_value = "",
        value_parser = |v: &str| { parse_address(v, PUBLIC_STUN_SERVER)},
        help = "The address of the stun server to use."
    )]
    stun_server: (String, u16),

    // Only to test the STUN response, the NAT traversal is an IPv4 problem
    // since the IPv6 is the identity function on a NAT:
    // ┌──────────────────┬────────────────────────────────┬────────────────────────────┐
    // │                  │           IPv4 + NAT           │            IPv6            │
    // ├──────────────────┼────────────────────────────────┼────────────────────────────┤
    // │ Address mapping  │ rewrites ip:port, table-driven │ none, identity             │
    // ├──────────────────┼────────────────────────────────┼────────────────────────────┤
    // │ Packet filtering │ derived from the mapping table │ explicit stateful firewall │
    // └──────────────────┴────────────────────────────────┴────────────────────────────┘
    #[clap(long, default_value = "false", help = "If we use an IPv6 socket.")]
    ipv6: bool,
}

/// One datagram exchanged between the two peers: a ping carrying its sequence
/// number so the receiver can detect loss and reordering on the wire.
#[derive(Debug, Serialize, Deserialize)]
struct Ping {
    side: comm::Side,
    seq: u32,
}

/// Build a simple terminal logger: human-readable, timestamped, async.
fn build_logger() -> Logger {
    let decorator = slog_term::TermDecorator::new().build();
    let drain = slog_term::FullFormat::new(decorator).build().fuse();
    let drain = slog_async::Async::new(drain)
        .chan_size(10000)
        .overflow_strategy(slog_async::OverflowStrategy::Block)
        .build()
        .fuse();

    Logger::root(drain, o!())
}

fn main() -> Result<()> {
    let args = Args::parse();
    let log = build_logger();

    info!(log, "Starting up"; "side" => ?args.side);
    info!(log, "Stun server: '{:?}'", args.stun_server);

    let socket = start_socket(&log, args.ipv6, args.side)?;
    let (ip, port) = send_stun_binding_request(&log, args.ipv6, &socket, PUBLIC_STUN_SERVER)?;
    info!(log, "Discovered own address over STUN: {ip}:{port}");

    ping_pong(&log, args.ipv6, args.side, &socket)?;

    Ok(())
}

fn start_socket(log: &Logger, ipv6: bool, side: comm::Side) -> Result<net::UdpSocket> {
    // NOTE: We must not take 127.0.0.1 otherwise we cannot
    // send the STUN binding response to on off-host address.
    // cause that IP is the loop back device.
    // Instead let the kernel choose the IP.
    // The kernel picks a source IP per route, not per destination.
    // Every destination:
    // - STUN server,
    // - out to other peer,
    // resolves to the same default route, same interface, same source IP.
    // Two different destinations, one source address.
    // And the part that actually matters for NAT traversal is the source port.
    // So the kernel uses port 10010 for every packet from that socket
    // regardless of destination.
    // It does not re-pick a port per peer. That's exactly the invariant "use one socket" is protecting.
    //
    // NOTE: The socket has to talk to two different peers over its lifetime:
    // STUN server, and the other side of the ping-pong.
    // a `socket.connect()` does not make sense and anyway is a sole kernel
    // operation on UDP sockets.
    //
    let our_addr: SocketAddr = if ipv6 {
        (Ipv6Addr::UNSPECIFIED, side.port()).into()
    } else {
        (Ipv4Addr::UNSPECIFIED, side.port()).into()
    };

    info!(log, "Binding UDP socket"; "address" => &our_addr);

    let socket = net::UdpSocket::bind(our_addr)
        .with_context(|| format!("Cannot bind socket on {our_addr}"))?;

    socket.set_read_timeout(Some(Duration::from_millis(500)))?;

    Ok(socket)
}

fn wait_for_sync_point(log: &Logger) {
    let time_frame = 10.0;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("could not get time")
        .as_secs_f64();

    let rest = time_frame - (now.rem_euclid(time_frame));

    info!(log, "Waiting {rest} secs to next sync point.");

    sleep(Duration::from_secs_f64(rest));
    info!(log, "Starting");
}

#[allow(clippy::similar_names)]
fn ping_pong(log: &Logger, ipv6: bool, side: comm::Side, sock: &net::UdpSocket) -> Result<()> {
    wait_for_sync_point(log);
    let other_addr: SocketAddr = if ipv6 {
        (Ipv6Addr::LOCALHOST, side.other().port()).into()
    } else {
        (Ipv4Addr::LOCALHOST, side.other().port()).into()
    };

    let mut data = vec![];

    for i in 0..10 {
        let ping = Ping { seq: i, side };

        data.clear();
        data = postcard::to_extend(&ping, data).context("could not serialize")?;
        info!(log, "Sending ping."; "i" => i);
        match sock.send_to(&data, other_addr) {
            Ok(s) if s != data.len() => warn!(log, "could not send datagram"),
            Err(e) => warn!(log, "could not send: {e}"),
            Ok(_) => {}
        }

        info!(&log, "Receiving ping: {i}");
        match sock.recv_from(&mut data) {
            Ok((b, addr)) => {
                if addr != other_addr {
                    warn!(log, "receive from unknown source '{addr}'");
                    continue;
                }
                if b == 0 {
                    warn!(log, "receive no bytes from '{addr}'");
                    continue;
                }
            }
            Err(e) => {
                warn!(log, "receive failed: {e}");
                continue;
            }
        }

        let recv = match postcard::from_bytes::<Ping>(&data) {
            Ok(p) => p,
            Err(e) => {
                warn!(log, "could not decode msg: {e}");
                continue;
            }
        };

        info!(log, "Received: {recv:?}");
        if recv.seq != i {
            warn!(log, "Message is not in sequence {} != {i}.", recv.seq);
        }
    }

    Ok(())
}
