use tokio::net::{UdpSocket, lookup_host};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

const STUN_SERVER: &str = "stun.l.google.com:19302";
const MAGIC_COOKIE: u32 = 0x2112A442;

pub async fn get_public(socket: &UdpSocket) -> Result<SocketAddr, Box<dyn std::error::Error>> {
    let stun_addr = resolve_stun_server().await.ok_or("Failed to resolve STUN server")?;
    // println!("Using STUN server: {}", stun_addr);

    let request = create_stun_request();
    socket.send_to(&request, stun_addr).await?;

    let mut buf = [0u8; 128];
    let (size, _src_addr) = socket.recv_from(&mut buf).await?;


    if let Some(public_ip) = parse_stun_response(&buf[..size]) {
        Ok(public_ip)
    } else {
        Err("Failed to retrieve public IP.".into())
    }
}


/// Resolves a hostname to an IPv4 STUN server address
async fn resolve_stun_server() -> Option<SocketAddr> {
    let addrs = lookup_host(STUN_SERVER).await.unwrap();
    
    // Pick the first IPv4 address, fallback to any available one
    for addr in addrs {
        if let SocketAddr::V6(_) = addr {
            return Some(addr);
        }
    }

    None
}

/// Creates a valid STUN Binding Request packet (RFC 5389)
fn create_stun_request() -> Vec<u8> {
    let mut packet = vec![0u8; 20];

    // STUN Binding Request Type (0x0001)
    packet[0] = 0x00;
    packet[1] = 0x01;

    // Message Length (0x0000 since no attributes are included)
    packet[2] = 0x00;
    packet[3] = 0x00;

    // Magic Cookie (0x2112A442)
    packet[4..8].copy_from_slice(&MAGIC_COOKIE.to_be_bytes());

    // Random 12-byte Transaction ID
    for i in 8..20 {
        packet[i] = rand::random::<u8>();
    }

    packet
}

/// Parses STUN response and extracts public IP
fn parse_stun_response(response: &[u8]) -> Option<SocketAddr> {
    if response.len() < 28 {
        return None;
    }

    let attr_type = u16::from_be_bytes([response[20], response[21]]);
    // let attr_len = u16::from_be_bytes([response[22], response[23]]);
    // println!("Attr type: {}, Attr len: {}", attr_type, attr_len);
    
    // Check if the attribute is XOR-MAPPED-ADDRESS (0x0020)
    if attr_type == 0x0020 {
        let family = response[25];
        let port = ((response[26] ^ response[4]) as u16) << 8 | (response[27] ^ response[5]) as u16;
        // println!("{:?}", &response[20..]);

        if family == 0x01 {
            let ip = Ipv4Addr::new(
                response[28] ^ response[4],
                response[29] ^ response[5],
                response[30] ^ response[6],
                response[31] ^ response[7]
            );

            return Some(SocketAddr::new(IpAddr::V4(ip), port));
        }

        if family == 0x02 {
            let mut ip = vec![];
            for i in 0..16 {
                ip.push(response[28 + i] ^ response[4 + i]);
            }
            return Some(SocketAddr::new(IpAddr::V6(
                Ipv6Addr::new(
                    u16::from_be_bytes([ip[0], ip[1]]),
                    u16::from_be_bytes([ip[2], ip[3]]),
                    u16::from_be_bytes([ip[4], ip[5]]),
                    u16::from_be_bytes([ip[6], ip[7]]),
                    u16::from_be_bytes([ip[8], ip[9]]),
                    u16::from_be_bytes([ip[10], ip[11]]),
                    u16::from_be_bytes([ip[12], ip[13]]),
                    u16::from_be_bytes([ip[14], ip[15]])
                )
            ), port));
        }
    }

    None
}

