use socket2::{Domain, Protocol, Socket, Type};
use std::fs::File;
use std::io;
use std::io::BufWriter;
use std::io::prelude::*;
use std::net::{Ipv4Addr, SocketAddr};

struct ErrorEvent {
    id: u64,
    seq: u64,
    expected: u64,
    history: [([u8; 8], [u8; 8]); 10],
}

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
    let mut out_of_seq: i32 = 0;
    let mut last_miss: Vec<usize> = Vec::new();

    loop {
        let (len, _) = socket.recv_from(&mut buf).await?;

        if len < 20 {
            println!("Packet too small!");
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

        // Check if we wrapped around and caught up to a missed packet index
        last_miss.retain(|&missed_index| {
            if missed_index == i {
                println!(
                    "[ERROR] Didn't receive a compensating packet for index {}!",
                    missed_index
                );
                false // Return false to REMOVE this element from the vector
            } else {
                true // Return true to KEEP this element
            }
        });

        if seq_num > expected_seq {
            println!("[LOG] PACKET DROPPED? Skipped a sequence number. No Error Yet");
            expected_seq = seq_num + 1;
            out_of_seq += seq_num as i32 - expected_seq as i32;
            last_miss.push(i);
        } else if seq_num < expected_seq {
            println!("[LOG] Early sequence. Packet arrived late, or duplicated. No Error Yet");
            expected_seq = seq_num + 1;
            // Handle compensation and removal of last miss
            out_of_seq -= 1;
            if out_of_seq < 0 {
                println!("[ERROR] of duplicated packets!");
            }
        } else {
            println!(
                "[SUCCESS] Received Packet | ID: {} | Size: {} bytes | Sequence: {}",
                id_num, len, seq_num
            );
            expected_seq = seq_num + 1;
            continue;
        }
        let f = File::create(format!("error_history{}", error_counter))?;

        let mut writer = BufWriter::new(f);
        for c in 0..10 {
            writer.write_all(&history_buffer[(c + i) % 10].0)?;
        }

        error_counter += 1;
    }
}
