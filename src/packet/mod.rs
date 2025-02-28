mod chat;
pub mod file;

use crate::ReceiverRes;

use super::user::User;
use chat::ChatPacket;
use file::{AckPacket, FileMetadata, FilePacket, MetadataRes};
use serde::{Deserialize, Serialize};
use std::{io::{self, Write}, net::SocketAddr, sync:: Arc, time::Duration};
use tokio::{net::UdpSocket, sync::Mutex, time::timeout};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum Packet {
    Bind(BindingPacket),
    File(FilePacket),
    Chat(ChatPacket),
    Ack(AckPacket),
    Discovery(bool),
    Metadata(FileMetadata),
    MdRes(MetadataRes),
}

impl Packet {
    pub fn chat_packet(username: String, message: String) -> Self {
        Packet::Chat(ChatPacket::new(username, message))
    }

    pub fn create_ackpacket(chunk: usize) -> Self {
        Packet::Ack(AckPacket::new( chunk))
    }

    pub fn create_file_res(file: FileMetadata) -> Self {
        Packet::MdRes(MetadataRes::new(file))
    }

    pub fn create_filemetadata(filename: String, chunks: usize) -> Self {
        Packet::Metadata(FileMetadata::new(filename, chunks))
    }

    pub fn create_file_packet(filename: String, chunk_index: usize, total_chunks: usize, data: Vec<u8>) -> Self {
        Packet::File(FilePacket::new_chunk(filename, chunk_index, total_chunks, data))
    }

    pub fn create_binding_req(v: bool, name: String) -> Self {
        Packet::Bind(BindingPacket::new(true, v, name))
    }

    pub fn create_binding_res(v: bool, name: String) -> Self {
        Packet::Bind(BindingPacket::new(false, v, name))
    }

    pub fn _create_discover(v: bool) -> Self {
        Packet::Discovery(v)
    }

    pub fn serialize(&self) -> Vec<u8> {
        bincode::serialize(self).expect("failed to Serialize packet")
    }

    pub fn deserialize(bytes: &[u8]) -> Option<Self> {
        bincode::deserialize(bytes).ok()
    }

    pub async fn send_packet(
        &self,
        socket: &UdpSocket,
        peer: &SocketAddr,
    ) -> tokio::io::Result<()> {
        let data = self.serialize();
        socket.send_to(&data, peer).await?;
        Ok(())
    }

    pub async fn handle_binding(
        &self,
        socket: &UdpSocket,
        user_lock: Arc<Mutex<User>>,
        addr: SocketAddr,
        res_rx: ReceiverRes,
    ) {

        if let Packet::Bind(bind) = self {
            if bind.req {
                bind.handle_binding_req(socket, addr, user_lock, res_rx).await;
            } else {
                bind.handle_binding_res(addr, user_lock).await;
            }
        }
    }

    
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BindingPacket {
    pub req: bool,
    pub accept: bool,
    pub name: String,
}

impl BindingPacket {
    pub fn new(req: bool, accept: bool, name: String) -> Self {
        BindingPacket { req, accept, name }
    }

    async fn handle_binding_req(&self, socket: &UdpSocket, addr: SocketAddr, user_lock: Arc<Mutex<User>>,  mut res_rx: ReceiverRes) {
        if self.accept {
            user_lock.lock().await.req_res();
            print!("Connection req from {} : [y/n] -> ", self.name);
            io::stdout().flush().unwrap();
            let mut res = false;
            let mut count = 1;
            loop {
                if let Ok(Ok(input)) = timeout(Duration::from_secs(5), res_rx.recv()).await {
                    let ans = input.trim().chars().nth(0).unwrap().to_lowercase().to_string();
                    if ans == 'y'.to_string() {
                        res = true;
                        println!("Peer connected: {}", self.name);
                        break;
                    } else if ans == 'n'.to_string() || count >= 3 {
                        println!("Connection Denied");
                        break;
                    }
                }
                count += 1;
                print!("Something Went Wrong, \nTry Again: [y/n] -> ");
                io::stdout().flush().unwrap();
            }
            let mut user = user_lock.lock().await;
            user.req_resolve();
            if res { user.add_peer(addr, self.name.clone()); }
            let packet = Packet::create_binding_res(res, user.get_name());
            drop(user);
            if let Err(e) = packet.send_packet(socket, &addr).await {
                println!("Error in sending binding response {:?}", e);
            }
        } else {
            let mut user = user_lock.lock().await;
            user.remove_peer(addr);
            println!("Peer Disconnected {}", addr);
            let packet = Packet::create_binding_res(false, user.get_name());
            drop(user);
            if let Err(e) = packet.send_packet(socket, &addr).await {
                println!("Error in sending binding response {:?}", e);
            }
        }
    }

    async fn handle_binding_res(&self, addr: SocketAddr, user_lock: Arc<Mutex<User>>) {
        let mut user = user_lock.lock().await;
        if self.accept {
            user.add_peer(addr, self.name.clone());
            println!("Connected to {}", self.name);
        } else {
            user.remove_peer(addr);
            println!("Peer Disconnected {}", self.name);
        }
    }

}


