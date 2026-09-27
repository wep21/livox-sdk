//! Synchronous IPv4 UDP mechanics; no device behavior or file parsing.
#![forbid(unsafe_code)]
use std::{
    io,
    net::{Ipv4Addr, SocketAddr, SocketAddrV4, UdpSocket},
};
#[derive(Debug, Clone, Copy)]
pub struct Ports {
    pub discovery: u16,
    pub command: u16,
    pub point: u16,
    pub status: u16,
    pub imu: u16,
}
impl Default for Ports {
    fn default() -> Self {
        Self {
            discovery: 56000,
            command: 56100,
            point: 56300,
            status: 56200,
            imu: 56400,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandChannel {
    Discovery,
    Control,
}
pub struct UdpTransport {
    discovery: UdpSocket,
    command: UdpSocket,
    point: UdpSocket,
    ports: Ports,
}
impl UdpTransport {
    /// Bind to a concrete local device IP. Port zero is useful in local tests.
    pub fn bind(ip: Ipv4Addr, mut ports: Ports) -> io::Result<Self> {
        let bind = |port| -> io::Result<UdpSocket> {
            let s = UdpSocket::bind((ip, port))?;
            s.set_nonblocking(true)?;
            Ok(s)
        };
        let discovery = bind(ports.discovery)?;
        let command = bind(ports.command)?;
        let point = bind(ports.point)?;
        ports.discovery = discovery.local_addr()?.port();
        ports.command = command.local_addr()?.port();
        ports.point = point.local_addr()?.port();
        Ok(Self {
            discovery,
            command,
            point,
            ports,
        })
    }
    pub fn ports(&self) -> Ports {
        self.ports
    }
    fn socket(&self, c: CommandChannel) -> &UdpSocket {
        match c {
            CommandChannel::Discovery => &self.discovery,
            CommandChannel::Control => &self.command,
        }
    }
    pub fn receive(
        &self,
        channel: CommandChannel,
        buffer: &mut [u8],
    ) -> io::Result<Option<(usize, SocketAddrV4)>> {
        match self.socket(channel).recv_from(buffer) {
            Ok((n, SocketAddr::V4(peer))) => Ok(Some((n, peer))),
            Ok(_) => Ok(None),
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                ) =>
            {
                Ok(None)
            }
            Err(e) => Err(e),
        }
    }
    pub fn reply(
        &self,
        channel: CommandChannel,
        bytes: &[u8],
        peer: SocketAddrV4,
    ) -> io::Result<()> {
        send(self.socket(channel), bytes, peer)
    }
    pub fn send_point(&self, bytes: &[u8], peer: SocketAddrV4) -> io::Result<()> {
        send(&self.point, bytes, peer)
    }
}
fn send(socket: &UdpSocket, bytes: &[u8], peer: SocketAddrV4) -> io::Result<()> {
    if socket.send_to(bytes, peer)? != bytes.len() {
        return Err(io::Error::new(io::ErrorKind::WriteZero, "partial UDP send"));
    }
    Ok(())
}
