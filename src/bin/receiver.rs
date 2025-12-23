use socket2::{Domain, Protocol, Socket, Type};
use std::io;
use std::net::{Ipv4Addr, SocketAddr};

// Helper to set up the "Weird" Multicast Socket
fn new_multicast_socket(addr: SocketAddr) -> io::Result<std::net::UdpSocket> {
    let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;

    // CRITICAL: Allow multiple apps to listen to this port
    socket.set_reuse_address(true)?;
    #[cfg(unix)] // Only Unix (Linux/Mac) has reuse_port
    socket.set_reuse_port(true)?;

    // Bind to the port (5000), NOT the specific IP yet
    let bind_addr = SocketAddr::new("0.0.0.0".parse().unwrap(), addr.port());
    socket.bind(&bind_addr.into())?;

    // Join the "Radio Station" (233.0.0.1)
    if let std::net::IpAddr::V4(ipv4) = addr.ip() {
        socket.join_multicast_v4(&ipv4, &Ipv4Addr::UNSPECIFIED)?;
    }

    Ok(socket.into())
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let addr = "233.0.0.1:5000".parse().unwrap();

    // 1. Setup the raw socket
    let std_sock = new_multicast_socket(addr)?;
    std_sock.set_nonblocking(true)?;

    // 2. Upgrade to Tokio Async Socket
    let socket = tokio::net::UdpSocket::from_std(std_sock)?;

    println!("FluxProbe Listening on {}...", addr);

    let mut buf = [0u8; 1024];

    loop {
        let (len, _) = socket.recv_from(&mut buf).await?;

        if len < 20 {
            eprintln!("Packet too small!");
            continue;
        }

        // --- MANUAL PARSING (The "Zero Copy" way) ---
        // 1. Skip 10 bytes (Session ID)
        // 2. Read next 8 bytes as u64
        let seq_bytes: [u8; 8] = buf[10..18].try_into().unwrap();
        let seq_num = u64::from_be_bytes(seq_bytes);

        println!(
            "Received Packet | Size: {} bytes | Sequence: {}",
            len, seq_num
        );
    }
}
