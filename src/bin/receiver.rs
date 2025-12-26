use socket2::{Domain, Protocol, Socket, Type};
use std::fs::File;
use std::io;
use std::io::BufWriter;
use std::io::prelude::*;
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
    let mut expected_seq = 1u64;

    // Holds 10 packets, each of size 20 bytes
    // storing only 16 bytes for ease of u64 and testing
    let mut history_buffer = [([0u8; 8], [0u8; 8]); 10];
    let mut i = 0;
    let mut error_counter = 0;

    loop {
        let (len, _) = socket.recv_from(&mut buf).await?;

        if len < 20 {
            eprintln!("Packet too small!");
            continue;
        }

        // --- MANUAL PARSING (The "Zero Copy" way) ---
        // 1. Skip 10 bytes (Session ID)
        // 2. Read next 8 bytes as u64
        // ONLY FIRST 8 BITS ARE SCANNED
        let id_bytes: [u8; 8] = buf[2..10].try_into().unwrap();
        let id_num = u64::from_be_bytes(id_bytes);

        let seq_bytes: [u8; 8] = buf[10..18].try_into().unwrap();
        let seq_num = u64::from_be_bytes(seq_bytes);

        history_buffer[i] = (id_bytes, seq_bytes);
        i = (i + 1) % history_buffer.len();

        if seq_num > expected_seq {
            println!("PACKET DROPPED? Skipped a sequence number");
        } else if seq_num < expected_seq {
            println!("Early sequence. Packet arrived late, or duplicated.");
        } else {
            println!(
                "Received Packet | ID: {} | Size: {} bytes | Sequence: {}",
                id_num, len, seq_num
            );
            expected_seq = seq_num + 1;
            continue;
        }
        let f = File::create(format!("error_history{}", error_counter))?;

        let mut writer = BufWriter::new(f);
        for i in 0..10 {
            writer.write_all(&id_bytes)?;
        }

        error_counter += 1;
    }
}
