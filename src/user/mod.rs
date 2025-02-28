mod command;
pub mod peer;


use crate::ReceiverAck;

use super::packet::Packet;
use command::Command;
use peer::Peer;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::{IpAddr, Ipv6Addr};
use std::sync::Arc;
use std::{collections::HashSet, net::SocketAddr};
use tokio::net::UdpSocket;
use tokio::sync::Mutex;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct User {
    name: String,
    connected: HashSet<Peer>,
    ip_to_peer: HashMap<SocketAddr, Peer>,
    chat_on: bool,
    res: bool,
}

impl User {
    pub fn new(name: String) -> Self {
        let user = User {
            name,
            connected: HashSet::new(),
            ip_to_peer: HashMap::new(),
            chat_on: false,
            res: false,
        };
        user
    }

    pub fn req_res(&mut self) {
        self.res = true;
    }

    pub fn get_res(&self) -> bool {
        self.res
    }

    pub fn req_resolve(&mut self) {
        self.res = false;
    }

    pub fn toggle_chat(&mut self) -> bool {
        let v = self.chat_on;
        self.chat_on = !v;
        !v
    }

    pub fn get_name(&self) -> String {
        self.name.clone()
    }

    pub fn add_peer(&mut self, addr: SocketAddr, name: String) {
        let peer = Peer::new(name, addr);
        self.connected.insert(peer.clone());
        self.ip_to_peer.insert(addr, peer);
    }
    pub fn remove_peer(&mut self, addr: SocketAddr) {
        if let Some(peer) = self.ip_to_peer.get(&addr) {
            self.connected.remove(peer);
        }
        self.ip_to_peer.remove(&addr);
    }

    fn display_members(&self) {
        for peer in self.connected.iter() {
            println!("{} -> Port: {}", peer.get_name(), peer.get_port());
        }
    }

    pub async fn handle_input(
        &mut self,
        socket: Arc<UdpSocket>,
        buf: String,
        user_lock: Arc<Mutex<User>>,
        ack_rx: ReceiverAck,
    ) {
        let cmd = buf.split_once(":");
        match cmd {
            Some(("connect", addrstr)) => {
                if let Some(addr) = base58_to_addr(addrstr.trim().to_string()) {
                    let cnt = Command::Connect(addr);
                    cnt.handle_connect(&socket, self.get_name()).await;
                } else {
                    println!("Error parsing the ip addrs")
                }
            }
            Some(("members", _)) => self.display_members(),

            Some(("chat", _)) => {
                let mut lock = user_lock.lock().await;
                if lock.toggle_chat() {
                    println!("Chat Started");
                } else {
                    println!("Chat Stopped");
                }
            }

            Some(("file", arg)) => {
                let mut path = String::with_capacity(arg.len());
                let mut flag = false;
                for i in arg.trim().chars() {
                    if i == '\'' || i == '\"' {
                        if !flag {
                            path.clear();
                            flag = true;
                        }
                        continue;
                    }
                    path.push(i);
                }
                let cmd = Command::File(path);
                // println!("{:?}", path);
                if let Err(e) = cmd.read_file(socket, self, ack_rx).await {
                    println!("Error in File handeling, {}", e);
                }
            },

            Some(_) => println!("Not ImpleMented Yet"),

            None => {
                if self.chat_on {
                    let chat = Packet::chat_packet(self.get_name(), buf);
                    for peer in self.connected.iter() {
                        if let Err(e) = chat.send_packet(&socket, &peer.get_addr()).await {
                            println!("Error sending {}, {}", peer.get_name(), e);
                        }
                    }
                } else {
                    println!("Not a feature");
                }
            },
        }
    }
}

fn base58_to_addr(addr: String) -> Option<SocketAddr> {
    let (ip, port) = addr.split_once("/")?;

    let ip_bytes = bs58::decode(ip).into_vec().ok()?;
    if ip_bytes.len() != 16 {
        println!("Invalid IPv6 address length");
        return None;
    }
    let port_bytes = bs58::decode(port).into_vec().ok()?;
    if port_bytes.len() != 2 {
        println!("Invalid port length");
        return None;
    }

    let ipv6 = Ipv6Addr::from(<[u8; 16]>::try_from(ip_bytes).ok()?);
    let port = u16::from_be_bytes(<[u8; 2]>::try_from(port_bytes).ok()?);
    Some(SocketAddr::new(IpAddr::V6(ipv6), port))
}
