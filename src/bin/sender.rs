use rand::prelude::*;
use std::net::UdpSocket;
use std::thread;
use std::time::Duration;

fn main() -> std::io::Result<()> {
    // 1. Bind to an ephemeral port (0) allows the OS to pick one.
    let socket = UdpSocket::bind("0.0.0.0:0")?;

    // 2. The "Radio Station" address.
    // 233.0.0.1 is a standard multicast IP range.
    let multicast_addr = "233.0.0.1:5000";

    println!("Blasting market data to {}...", multicast_addr);

    let mut sequence_num = 1u64;

    loop {
        // --- MOLDUDP64 PACKET STRUCTURE (20 Bytes) ---
        // Session ID (10 bytes) | Sequence (8 bytes) | Count (2 bytes)
        let choice: f64 = rand::rng().random();
        let choice = (choice * 10.0) as u16;
        if choice == 0 {
            sequence_num += 1;
            continue;
        }

        let mut packet = Vec::with_capacity(20);

        // Session ID (just 10 bytes of zeros for now)
        let mut session_id = [0u8; 10];
        session_id[9] = 1;
        packet.extend_from_slice(&session_id);

        // Sequence Number (Critical: Big Endian)
        packet.extend_from_slice(&sequence_num.to_be_bytes());

        // Message Count (1 message)
        packet.extend_from_slice(&1u16.to_be_bytes());

        // Send it!
        socket.send_to(&packet, multicast_addr)?;

        println!("Sent Sequence: {}", sequence_num);
        sequence_num += 1;

        // Sleep 1 second (slow enough to see with your eyes)
        thread::sleep(Duration::from_secs(1));
    }
}
