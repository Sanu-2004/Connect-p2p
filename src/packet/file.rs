use indicatif::{ProgressBar, ProgressState, ProgressStyle};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use std::io::{self, Write};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;
use tokio::net::UdpSocket;
use tokio::time::timeout;

use crate::user::User;
use crate::ReceiverRes;

use super::Packet;

const PACKET_SIZE: usize = 65 * 1024;
const FILE_TIMEOUT: Duration = Duration::from_secs(4);

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct FilePacket {
    pub filename: String,
    pub filesize: usize,
    pub chunk_index: usize,
    pub total_chunks: usize,
    pub data: Vec<u8>,
    pub hash: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct FileMetadata {
    pub filename: String,
    pub total_chunks: usize,
    pub key: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MetadataRes {
    pub total_chunks: usize,
    pub key: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AckPacket {
    pub chunk_index: usize,
}

impl AckPacket {
    pub fn new(chunk_index: usize) -> Self {
        AckPacket { chunk_index }
    }
}

impl MetadataRes {
    pub fn new(file: FileMetadata) -> Self {
        MetadataRes {
            total_chunks: file.total_chunks,
            key: file.key,
        }
    }

    pub fn verify(&self, packet: &Packet) -> bool {
        if let Packet::Metadata(file) = packet {
            return (self.key == file.key) && (self.total_chunks == file.total_chunks);
        }
        false
    }
}

impl FileMetadata {
    pub fn new(filename: String, total_chunks: usize) -> Self {
        let mut hasher = blake3::Hasher::new();
        hasher.update(filename.as_bytes());
        let key = hasher.finalize().to_string();
        FileMetadata {
            filename,
            total_chunks,
            key,
        }
    }

    pub async fn receive_file(
        &self,
        socket: &UdpSocket,
        addr: SocketAddr,
        user_lock: Arc<Mutex<User>>,
        mut res_rx: ReceiverRes,
    ) -> std::io::Result<()> {
        user_lock.lock().await.req_res();
        print!("File Sending req from {} : [y/n] -> ", self.filename);
        io::stdout().flush().unwrap();
        let mut res = false;
        loop {
            if let Ok(Ok(input)) = timeout(FILE_TIMEOUT, res_rx.recv()).await {
                let ans = input
                    .trim()
                    .chars()
                    .nth(0)
                    .unwrap()
                    .to_lowercase()
                    .to_string();
                if ans == 'y'.to_string() {
                    res = true;
                    break;
                } else if ans == 'n'.to_string(){
                    break;
                }
            } else {
                break;
            }
        }
        user_lock.lock().await.req_resolve();

        if !res {
            println!("Connection Denied");
            return Ok(())
        }

        let packet = Packet::create_file_res(self.clone());
        packet.send_packet(socket, &addr).await?;

        let pb = ProgressBar::new(self.total_chunks as u64 * 59 * 1024);
        pb.set_style(ProgressStyle::with_template("{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {bytes}/{total_bytes} ({eta})")
          .unwrap()
          .with_key("eta", |state: &ProgressState, w: &mut dyn std::fmt::Write| write!(w, "{:.1}s", state.eta().as_secs_f64()).unwrap())
          .progress_chars("#>-"));

        let mut buf = [0; PACKET_SIZE];
        let mut file = File::create(self.filename.clone()).await?;
        let mut index = 1;
        let mut attempt = 0;
        loop {
            if attempt >= 3 {
                break;
            }
            match timeout(FILE_TIMEOUT, socket.recv_from(&mut buf)).await {
                Ok(Ok((size, _))) => {
                    if let Some(packet) = Packet::deserialize(&buf[..size]) {
                        match packet {
                            Packet::File(f) => {
                                if (f.chunk_index == index) && f.verify_chunk() {
                                    if let Err(e) = file.write_all(&f.data).await {
                                        println!("Error writing file. \n{}", e);
                                        attempt += 1;
                                        continue;
                                    }
                                    index += 1;
                                    pb.set_position(index as u64 * 59 * 1024);
                                } else {
                                    println!("f.chunk_index==index: {}", f.chunk_index == index);
                                    println!("file verification: {}", f.verify_chunk());
                                }
                            }
                            _ => println!("Other Peer Want to contact"),
                        }
                    }
                }
                Ok(Err(e)) => {
                    println!("Error getting chunk index {} /retrying \n{}", index, e);
                    attempt += 1;
                }
                Err(_) => {
                    println!("Reciving window timeout chunk index {} /retrying", index,);
                    attempt += 1;
                }
            }
            let ack = Packet::create_ackpacket(index);
            if let Err(e) = ack.send_packet(socket, &addr).await {
                println!("Error sending ack for chunk {} \n{}", index, e);
            }
            if index > self.total_chunks {
                println!("Recived File");
                break;
            }
        }
        pb.finish_and_clear();
        Ok(())
    }
}

impl FilePacket {
    pub fn new_chunk(
        filename: String,
        chunk_index: usize,
        total_chunks: usize,
        data: Vec<u8>,
    ) -> Self {
        let mut hasher = blake3::Hasher::new();
        hasher.update(&data);
        let hash = Some(hasher.finalize().to_string());
        // let hash = None;

        FilePacket {
            filename,
            filesize: data.len(),
            chunk_index,
            total_chunks,
            data,
            hash,
        }
    }

    pub fn verify_chunk(&self) -> bool {
        if let Some(exp_hash) = &self.hash {
            let mut hasher = blake3::Hasher::new();
            hasher.update(&self.data);
            let computed_hash = hasher.finalize().to_string();
            computed_hash == *exp_hash
        } else {
            true
        }
    }
}
