//! Network-independent minimal Mid-360 command state machine.
#![forbid(unsafe_code)]
use livox_protocol::control::{self, DiscoveryResponse, HostEndpoint};
use livox_protocol::{
    CommandHeader, CommandPacket, CommandPacketOwned, CommandType, EncodeError, SenderType,
};
use std::net::Ipv4Addr;
#[derive(Debug, thiserror::Error)]
pub enum DeviceError {
    #[error("wire encode: {0}")]
    Wire(#[from] bincode::error::EncodeError),
    #[error("command encode: {0}")]
    Command(#[from] EncodeError),
}
#[derive(Debug, Clone)]
pub struct Mid360 {
    pub ip: Ipv4Addr,
    pub serial: [u8; 16],
    pub command_port: u16,
    pub point_format: u8,
    pub status_endpoint: HostEndpoint,
    pub point_endpoint: HostEndpoint,
    pub imu_endpoint: HostEndpoint,
    pub work_mode: u8,
}
impl Mid360 {
    pub fn sampling(&self) -> bool {
        self.work_mode == 1
    }
    pub fn discovery(&self, sequence: u32) -> Result<CommandPacketOwned, DeviceError> {
        self.ack(
            sequence,
            control::DISCOVERY,
            control::encode_fixed(&DiscoveryResponse {
                ret_code: 0,
                device_type: 9,
                serial: self.serial,
                ip: self.ip.octets(),
                command_port: self.command_port,
            })?,
        )
    }
    fn ack(
        &self,
        sequence: u32,
        cmd_id: u16,
        payload: Vec<u8>,
    ) -> Result<CommandPacketOwned, DeviceError> {
        Ok(CommandPacketOwned {
            header: CommandHeader {
                seq_num: sequence,
                cmd_id,
                cmd_type: CommandType::Ack,
                sender_type: SenderType::Lidar,
                reserved: [0; 6],
            },
            payload,
        })
    }
    /// State changes occur only after the entire parameter batch is validated.
    pub fn handle(
        &mut self,
        p: &CommandPacket<'_>,
    ) -> Result<Option<CommandPacketOwned>, DeviceError> {
        if p.header.cmd_type != CommandType::Request || p.header.sender_type != SenderType::Host {
            return Ok(None);
        }
        let response = match p.header.cmd_id {
            control::DISCOVERY if p.payload.is_empty() => {
                return Ok(Some(self.discovery(p.header.seq_num)?));
            }
            control::DISCOVERY => vec![0x23],
            control::SET_PARAMETERS => match control::decode_set(p.payload) {
                Err(livox_protocol::DecodeError::InvalidLength { .. }) => vec![0x24, 0, 0],
                Err(_) => vec![0x23, 0, 0],
                Ok(values) => {
                    let mut proposed = self.clone();
                    let mut seen = std::collections::BTreeSet::new();
                    let mut error = None;
                    for kv in values {
                        let result = if !seen.insert(kv.key) {
                            Err(0x24)
                        } else {
                            proposed.set(kv.key, kv.value)
                        };
                        if let Err(code) = result {
                            error = Some((code, kv.key));
                            break;
                        }
                    }
                    if let Some((code, key)) = error {
                        let mut v = vec![code];
                        v.extend(key.to_le_bytes());
                        v
                    } else {
                        *self = proposed;
                        vec![0, 0, 0]
                    }
                }
            },
            control::GET_PARAMETERS => match control::decode_get(p.payload) {
                Err(livox_protocol::DecodeError::InvalidLength { .. }) => vec![0x24, 0, 0],
                Err(_) => vec![0x23, 0, 0],
                Ok(keys) => {
                    let mut body = vec![0];
                    body.extend((keys.len() as u16).to_le_bytes());
                    let mut failure = None;
                    for key in keys {
                        match self.get(key)? {
                            None => {
                                failure = Some(0x20);
                                break;
                            }
                            Some(value) => {
                                if body.len() + 4 + value.len() > 1376 {
                                    failure = Some(0x23);
                                    break;
                                }
                                body.extend(key.to_le_bytes());
                                body.extend((value.len() as u16).to_le_bytes());
                                body.extend(value);
                            }
                        }
                    }
                    if let Some(code) = failure {
                        vec![code, 0, 0]
                    } else {
                        body
                    }
                }
            },
            _ => vec![0x20],
        };
        Ok(Some(self.ack(
            p.header.seq_num,
            p.header.cmd_id,
            response,
        )?))
    }
    fn set(&mut self, key: u16, value: &[u8]) -> Result<(), u8> {
        match key {
            0 => {
                if value.len() != 1 {
                    return Err(0x23);
                }
                if value[0] != self.point_format {
                    return Err(0x20);
                }
                Ok(())
            }
            5..=7 => {
                let target = match key {
                    5 => &mut self.status_endpoint,
                    6 => &mut self.point_endpoint,
                    _ => &mut self.imu_endpoint,
                };
                let endpoint = HostEndpoint::decode(value).map_err(|_| 0x23)?;
                let ip = Ipv4Addr::from(endpoint.ip);
                if endpoint.source_port != target.source_port {
                    return Err(0x20);
                }
                if endpoint.destination_port == 0
                    || ip.is_unspecified()
                    || ip.is_multicast()
                    || ip.is_broadcast()
                {
                    return Err(3);
                }
                *target = endpoint;
                Ok(())
            }
            0x1a => {
                if value.len() != 1 {
                    return Err(0x23);
                }
                if !matches!(value[0], 1 | 2 | 9) {
                    return Err(3);
                }
                self.work_mode = value[0];
                Ok(())
            }
            0x1c => {
                if value.len() != 1 {
                    return Err(0x23);
                }
                if value[0] > 1 {
                    Err(3)
                } else if value[0] == 1 {
                    Err(0x20)
                } else {
                    Ok(())
                }
            }
            0x8000..=0x8011 => Err(0x22),
            _ => Err(0x20),
        }
    }
    fn get(&self, key: u16) -> Result<Option<Vec<u8>>, DeviceError> {
        Ok(Some(match key {
            0 => vec![self.point_format],
            5 => control::encode_fixed(&self.status_endpoint)?,
            6 => control::encode_fixed(&self.point_endpoint)?,
            7 => control::encode_fixed(&self.imu_endpoint)?,
            0x1a | 0x8006 => vec![self.work_mode],
            0x1c => vec![0],
            0x8000 => self.serial.to_vec(),
            0x8001 => {
                let mut v = vec![0; 64];
                let text = b"Virtual Mid-360 (livox-rs)";
                v[..text.len()].copy_from_slice(text);
                v
            }
            0x8002 => vec![0, 0, 0, 1],
            0x800e => vec![0, 0],
            0x8010 => vec![1],
            _ => return Ok(None),
        }))
    }
}
