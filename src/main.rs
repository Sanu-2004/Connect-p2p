mod packet;
mod stun;
mod user;

pub type SenderAck = broadcast::Sender<Option<(Packet, SocketAddr)>>;
pub type ReceiverAck = broadcast::Receiver<Option<(Packet, SocketAddr)>>;
pub type ReceiverRes = broadcast::Receiver<String>;
pub type SenderRes = broadcast::Sender<String>;

use bs58;
use crossterm::{
    cursor::MoveTo,
    execute,
    style::{Color, Print, ResetColor, SetForegroundColor},
    terminal::{Clear, ClearType},
};
use packet::Packet;
use std::{
    io::{self, stdin, Write},
    net::{IpAddr, SocketAddr},
    sync::Arc,
};
use tokio::{
    net::UdpSocket,
    signal,
    sync::{broadcast, Mutex},
};
use user::User;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    print_heading();

    execute!(
        io::stdout(),
        SetForegroundColor(Color::Green),
        Print("Enter your public name: "),
        ResetColor
    )?;

    io::stdout().flush().unwrap();
    let mut name = String::with_capacity(50);
    loop {
        name.clear();
        stdin().read_line(&mut name)?;
        let trimmed = name.trim();

        if trimmed.is_empty() {
            execute!(
                io::stdout(),
                SetForegroundColor(Color::Red),
                Print("\n[!] Name cannot be empty! Please re-enter: "),
                ResetColor
            )?;
            io::stdout().flush().unwrap();
            continue;
        }
        break;
    }
    execute!(
        io::stdout(),
        SetForegroundColor(Color::Cyan),
        Print(format!("\n✅ Welcome, {}!\n", name.trim())),
        ResetColor
    )?;

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

        println!("\nYour Addr: {}/{}\n", addr, port);
        "Type 'help:' for help";
        execute!(
            io::stdout(),
            SetForegroundColor(Color::Cyan),
            Print(format!("type 'help:' for help\n\n")),
            ResetColor
        )?;
    }

    let socket_clone = socket.clone();
    tokio::spawn(sender(socket_clone, user_lock.clone(), tx, file_rx));
    tokio::spawn(handle_ctrl_c(user_lock.clone(), socket.clone()));

    let mut buf = [0; 2048];
    loop {
        match socket.recv_from(&mut buf).await {
            Ok((size, addr)) => {
                println!("{}", String::from_utf8_lossy(&buf[..size]));
                handle_message(
                    &socket,
                    user_lock.clone(),
                    &buf[..size],
                    addr,
                    res_rx.resubscribe(),
                    file_tx.clone(),
                )
                .await;
            },
            Err(e) => println!("Error reciving msg, {}", e),
        }
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
        match &packet {
            Packet::Chat(c) => c.display(),
            Packet::Bind(_) => {
                packet
                    .handle_binding(&socket, user_lock, addr, res_rx)
                    .await
            }
            Packet::Metadata(pac) => {
                if let Err(e) = pac.receive_file(&socket, addr, user_lock, res_rx).await {
                    println!("error reciving first file packet \n{}", e);
                }
            }
            Packet::MdRes(_) => {
                if let Err(e) = tx.send(Some((packet.clone(), addr))) {
                    eprintln!("Error transmitting MdRes packet: {}", e)
                }
            }
            Packet::Ack(_) => {
                if let Err(e) = tx.send(Some((packet.clone(), addr))) {
                    eprintln!("Error transmitting ack packet: {}", e)
                }
            }
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
        print!(">");
        io::stdout().flush().unwrap();
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
            continue;
        }
        user.handle_input(socket_clone, buf.clone(), user_lock.clone(), ack_rx)
            .await;
    }
}

async fn handle_ctrl_c(user_lock: Arc<Mutex<User>>, socket: Arc<UdpSocket>) {
    let user_lock_clone = user_lock.clone();
    tokio::spawn(async move {
        if let Err(e) = signal::ctrl_c().await {
            eprintln!("Error listening for shutdown signal: {}", e);
        }
        println!("\nExiting... Disconnecting peers.");

        let user = user_lock_clone.lock().await;
        user.disconnect_all(&socket).await;

        std::process::exit(0);
    });
}

fn print_heading() {
    let text = r"   ______                            __             ___       
  / ____/___  ____  ____  ___  _____/ /_      ____ |__ \ ____ 
 / /   / __ \/ __ \/ __ \/ _ \/ ___/ __/_____/ __ \__/ // __ \
/ /___/ /_/ / / / / / / /  __/ /__/ /_/_____/ /_/ / __// /_/ /
\____/\____/_/ /_/_/ /_/\___/\___/\__/     / .___/____/ .___/ 
                                          /_/        /_/         
 ";

    let github = "by Sanu-2004 (GitHub)\n\n";

    let mut stdout = io::stdout();

    execute!(
        stdout,
        Clear(ClearType::All),
        MoveTo(0, 0),
        SetForegroundColor(Color::Green),
        Print(text),
        ResetColor,
        SetForegroundColor(Color::Yellow),
        Print(github),
        ResetColor,
    )
    .unwrap();

    println!();
}
