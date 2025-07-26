use std::net::{UdpSocket, Ipv4Addr, SocketAddr};
use std::io::{self, Write};

// DNS Header structure
#[repr(C)]
struct DnsHeader {
    xid: u16,
    flags: u16,
    qdcount: u16,
    ancount: u16,
    nscount: u16,
    arcount: u16,
}

// DNS Record A structure
#[repr(C)]
struct DnsRecordAT {
    compression: u16,
    typ: u16,
    clas: u16,
    ttl: u32,
    length: u16,
    addr: u32, // IPv4 address as u32
}

fn main() -> io::Result<()> {
    // Create UDP socket
    let socket = UdpSocket::bind("0.0.0.0:0")?;
    println!("is sockfd valid? yes");

    // Create DNS header
    let mut dnsh = DnsHeader {
        xid: 0x1234,
        flags: 0x0100,
        qdcount: 0x0001,
        ancount: 0,
        nscount: 0,
        arcount: 0,
    };

    // Hostname for DNS query: engineering.nyu.edu
    let hostname = b"\x0bengineering\x03nyu\x03edu\x00";
    let dnstype: u16 = 1; // A record
    let dnsclass: u16 = 1; // IN class

    // Construct packet
    let packet_len = std::mem::size_of::<DnsHeader>() + hostname.len() + 2 + 2;
    let mut packet = vec![0u8; packet_len];
    
    // Copy DNS header
    let header_bytes = unsafe {
        std::slice::from_raw_parts(
            &dnsh as *const DnsHeader as *const u8,
            std::mem::size_of::<DnsHeader>()
        )
    };
    packet[..header_bytes.len()].copy_from_slice(header_bytes);
    
    // Copy hostname
    let hostname_start = std::mem::size_of::<DnsHeader>();
    packet[hostname_start..hostname_start + hostname.len()].copy_from_slice(hostname);
    
    // Copy query type and class
    let type_start = hostname_start + hostname.len();
    packet[type_start..type_start + 2].copy_from_slice(&dnstype.to_be_bytes());
    packet[type_start + 2..type_start + 4].copy_from_slice(&dnsclass.to_be_bytes());

    // Set up destination address (OpenDNS: 208.67.222.222:53)
    let dest_addr = SocketAddr::new(
        Ipv4Addr::new(208, 67, 222, 222).into(),
        53
    );

    // Send DNS query
    let sent_len = socket.send_to(&packet, dest_addr)?;
    println!("{} is the length sent", sent_len);

    // Receive DNS response
    let mut dns_resp = [0u8; 512];
    let (received_len, _) = socket.recv_from(&mut dns_resp)?;
    println!("{} is the length received", received_len);

    // Extract response header
    let response_header = unsafe {
        &*(dns_resp.as_ptr() as *const DnsHeader)
    };

    // Check if response is valid (no error flags)
    if (response_header.flags.to_be() & 0xf) != 0 {
        println!("DNS response contains error flags");
        return Ok(());
    }

    // Skip over the name in response
    let mut name_ptr = std::mem::size_of::<DnsHeader>();
    while name_ptr < received_len && dns_resp[name_ptr] != 0 {
        name_ptr += dns_resp[name_ptr] as usize + 1;
    }

    // Skip null byte, qtype, and qclass to get to answer section
    let record_ptr = name_ptr + 5;
    if record_ptr + std::mem::size_of::<DnsRecordAT>() <= received_len {
        let record = unsafe {
            &*(dns_resp.as_ptr().add(record_ptr) as *const DnsRecordAT)
        };
        
        // Convert IPv4 address to string
        let ip_bytes = record.addr.to_be_bytes();
        let ip_addr = Ipv4Addr::new(ip_bytes[0], ip_bytes[1], ip_bytes[2], ip_bytes[3]);
        println!("the dns lookup yields the ip address: {}", ip_addr);
    } else {
        println!("DNS response too short to extract answer");
    }

    Ok(())
} 