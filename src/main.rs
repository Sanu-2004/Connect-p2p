mod packet;
mod stun;
mod user;

pub type SenderAck = broadcast::Sender<Option<(Packet, SocketAddr)>>;
pub type ReceiverAck = broadcast::Receiver<Option<(Packet, SocketAddr)>>;
pub type ReceiverRes = broadcast::Receiver<String>;
pub type SenderRes = broadcast::Sender<String>;

use bs58;
use packet::Packet;
use std::{
    io::{self, Write, stdin}, net::{IpAddr, SocketAddr}, sync::
        Arc
};
use tokio::{
    net::UdpSocket,
    sync::{broadcast, Mutex},
};
use user::User;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    print!("Enter your public name : ");
    io::stdout().flush().unwrap();
    let mut name = String::with_capacity(50);
    loop {
        name.clear();
        stdin().read_line(&mut name)?;
        if name.trim() == "" {
            println!("Name Cannot be empty");
            print!("Plz Re-Enter Name: ");
            io::stdout().flush().unwrap();
            continue;
        }
        break;
    }

    let (tx, res_rx) = broadcast::channel(16);
    let (file_tx, file_rx) = broadcast::channel(16);

    let user = User::new(name.trim().to_string());
    let user_lock = Arc::new(Mutex::new(user));

    let socket = Arc::new(UdpSocket::bind("[::]:0").await?);
    let public = stun::get_public(&socket).await?;
    if let IpAddr::V6(ip) = public.ip() {
        let octet = ip.octets();
        let addr = bs58::encode(octet).into_string();
        let port = bs58::encode(u16::to_be_bytes(public.port())).into_string();
        println!("\nYour Public Addr: {}/{}\n", addr, port);
    }

    // Spawn a task to handle outgoing messages
    let socket_clone = socket.clone();
    tokio::spawn(sender(socket_clone, user_lock.clone(), tx, file_rx));

    let mut buf = [0; 2048];
    loop {
        let (size, addr) = socket.recv_from(&mut buf).await?;
        // let msg = String::from_utf8_lossy(&buf[..size]).to_string();
        // println!("{} : {}", addr, msg);
        handle_message(
            &socket,
            user_lock.clone(),
            &buf[..size],
            addr,
            res_rx.resubscribe(),
            file_tx.clone(),
        )
        .await;
    }
}

async fn handle_message(
    socket: &UdpSocket,
    user_lock: Arc<Mutex<User>>,
    bytes: &[u8],
    addr: SocketAddr,
    res_rx: ReceiverRes,
    tx: SenderAck,
) {
    if let Some(packet) = Packet::deserialize(bytes) {
        // let pac_clone = packet.clone();
        match &packet {
            Packet::Chat(c) => c.display(),
            Packet::Bind(_) => packet.handle_binding(&socket, user_lock, addr, res_rx).await,
            Packet::Metadata(pac) => {
                if let Err(e) = pac.receive_file(&socket, addr, user_lock, res_rx).await {
                    println!("error reciving first file packet \n{}", e);
                }
            },
            Packet::MdRes(_) => {
                if let Err(e) = tx.send(Some((packet.clone(), addr))) {
                    eprintln!("Error transmitting MdRes packet: {}", e)
                }
                
            },
            Packet::Ack(_) => {
                if let Err(e) = tx.send(Some((packet.clone(), addr))) {
                    eprintln!("Error transmitting ack packet: {}", e)
                }
            },
            _ => println!("working on this part"),
        }
    }
}

async fn sender(
    socket: Arc<UdpSocket>,
    user_lock: Arc<Mutex<User>>,
    tx: SenderRes,
    file_rx: ReceiverAck,
) {
    let mut buf = String::new();
    loop {
        let ack_rx = file_rx.resubscribe();
        let socket_clone = socket.clone();
        buf.clear();
        if stdin().read_line(&mut buf).is_err() {
            eprintln!("Failed to read from stdin");
            continue;
        }
        let mut user = {
            let lock = user_lock.lock().await;
            lock.clone()
        };
        if user.get_res() {
            if let Err(e) = tx.send(buf.clone()) {
                eprintln!("Error Sending res \n{}", e);
            }
            // {
            //     let mut lock = user_lock.lock().await;
            //     lock.req_resolve();
            // }
            continue;
        }
        user.handle_input(socket_clone, buf.clone(), user_lock.clone(), ack_rx)
            .await;
    }
}
