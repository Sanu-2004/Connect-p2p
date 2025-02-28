use std::net::{SocketAddr, IpAddr};
use serde::{Serialize, Deserialize};


#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Hash)]
pub struct Peer {
    name: String,
    addr: SocketAddr,
}

impl Peer {
    pub fn new(name: String, addr: SocketAddr) -> Self {
        Peer { name, addr }
    }

    pub fn get_name(&self) -> &str {
        &self.name
    }

    pub fn get_port(&self) -> u16 {
        self.addr.port()
    }

    pub fn _get_ip(&self) -> IpAddr {
        self.addr.ip()
    }

    pub fn get_addr(&self) -> SocketAddr {
        self.addr
    }
}