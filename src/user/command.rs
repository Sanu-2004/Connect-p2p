use indicatif::{MultiProgress, ProgressBar, ProgressState, ProgressStyle};
use std::{collections::HashSet, fmt::Write, net::SocketAddr, path::Path, sync::Arc, time::Duration};
use tokio::{
    fs::File,
    io::{AsyncReadExt, BufReader},
    net::UdpSocket,
    time::timeout,
};

use super::{Packet, ReceiverAck, User};

const CHUNK_SIZE: usize = 59 * 1024;

pub enum Command {
    Connect(SocketAddr),
    File(String),
}

impl Command {
    pub async fn handle_connect(&self, socket: &UdpSocket, name: String) {
        if let Command::Connect(addr) = self {
            let packet = Packet::create_binding_req(true, name);
            if let Err(e) = packet.send_packet(socket, addr).await {
                eprintln!("Error sending connection packet: {:?}", e);
            }
        } else {
            eprintln!("Invalid command: Expected `Connect`");
        }
    }

    pub async fn read_file(
        &self,
        socket: Arc<UdpSocket>,
        user: &mut User,
        mut ack_rx: ReceiverAck,
    ) -> tokio::io::Result<()> {
        let path = match self {
            Command::File(p) => p.to_string(),
            _ => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "Invalid command",
                ))
            }
        };

        let file = File::open(&path).await?;
        let metadata = file.metadata().await?;
        let total_size = metadata.len() as usize;
        let total_chunks = (total_size + CHUNK_SIZE - 1) / CHUNK_SIZE;

        let file_name = match Path::new(&path).file_name() {
            Some(name) => name.to_string_lossy().to_string(),
            None => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "Invalid file name",
                ))
            }
        };

        let packet = Packet::create_filemetadata(file_name.clone(), total_chunks);
        let mut interested_peer = HashSet::with_capacity(user.connected.len());

        let user_clone = user.clone();
        for peer in user_clone.connected {
            let socket_clone = socket.clone();
            let packet_clone = packet.clone();
            let _ = packet_clone
                .send_packet(&socket_clone, &peer.get_addr())
                .await;
        }

        loop {
            match timeout(Duration::from_secs(5), ack_rx.recv()).await {
                Ok(Ok(Some((pac, addr)))) => {
                    if let Packet::MdRes(res) = pac {
                        if res.verify(&packet) {
                            interested_peer.insert(addr);
                        }
                    }
                }
                _ => break,
            }
            if interested_peer.len() == user.connected.len() {
                break;
            }
        }

        if interested_peer.len() == 0 {
            println!("No Peer Responded");
            return Ok(());
        }
        println!("total {} peer responded", interested_peer.len());
        let mut tasks = vec![];

        let m = MultiProgress::new();
        let sty = ProgressStyle::with_template("{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {bytes}/{total_bytes} ({eta})")
        .unwrap()
        .with_key("eta", |state: &ProgressState, w: &mut dyn Write| write!(w, "{:.1}s", state.eta().as_secs_f64()).unwrap())
        .progress_chars("#>-");

        for i in interested_peer {
            let socket_clone = socket.clone();
            let file_name_clone = file_name.clone();
            let path_clone = path.clone();
            let res_rx = ack_rx.resubscribe();
            let pb = m.add(ProgressBar::new((total_chunks*CHUNK_SIZE) as u64));
            pb.set_style(sty.clone());
            pb.set_message("todo");
            let thread = tokio::spawn(async move {
                if let Err(e) = handle_peer(
                    &socket_clone,
                    total_chunks,
                    file_name_clone,
                    path_clone,
                    i,
                    res_rx,
                    pb
                )
                .await
                {
                    eprintln!("Err Sending file to peer, \n{}", e);
                }
            });
            tasks.push(thread);
        }

        for task in tasks {
            if let Err(e) = task.await {
                println!("Error compelting task, {}", e);
            }
        }
        println!("Sending Completed");

        Ok(())
    }
}

async fn handle_peer(
    socket: &UdpSocket,
    total_chunks: usize,
    file_name: String,
    path: String,
    addr: SocketAddr,
    mut ack_rx: ReceiverAck,
    pb: ProgressBar,
) -> tokio::io::Result<()> {
    let mut buf = [0; CHUNK_SIZE];
    
    let file = File::open(&path).await?;
    let mut reader = BufReader::new(file);
    let mut index = 1;
    let mut n = reader.read(&mut buf).await?;
    let mut attemp = 0;

    loop {
        if n == 0 || attemp >= 3 {
            break;
        }
        let packet =
            Packet::create_file_packet(file_name.clone(), index, total_chunks, buf[..n].to_vec());
        packet.send_packet(socket, &addr).await?;
        // println!("{}/{}", index, total_chunks);
        match timeout(Duration::from_secs(3), ack_rx.recv()).await {
            Ok(Ok(Some((pac, receiver_addr)))) => {
                if let Packet::Ack(ack) = pac {
                    if (ack.chunk_index == index + 1) && (addr == receiver_addr) {
                        index += 1;
                        pb.set_position((index*CHUNK_SIZE) as u64);
                        n = reader.read(&mut buf).await?;
                    }
                }
            }
            _ => {
                println!("Error getting ack");
                attemp += 1;
            }
        }
    }
    pb.finish_and_clear();
    Ok(())
}
