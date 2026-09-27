use livox_device::Mid360;
use livox_protocol::{CommandHeader, CommandPacket, CommandType, SenderType, control::*};
fn device() -> Mid360 {
    let endpoint = |source_port| HostEndpoint {
        ip: [127, 0, 0, 1],
        destination_port: source_port + 1,
        source_port,
    };
    Mid360 {
        ip: [127, 0, 0, 2].into(),
        serial: *b"VIRTUALMID36001\0",
        command_port: 56100,
        point_format: 1,
        status_endpoint: endpoint(56200),
        point_endpoint: endpoint(56300),
        imu_endpoint: endpoint(56400),
        work_mode: 2,
    }
}
fn request(command: u16, payload: &[u8]) -> CommandPacket<'_> {
    CommandPacket {
        header: CommandHeader {
            seq_num: u32::MAX,
            cmd_id: command,
            cmd_type: CommandType::Request,
            sender_type: SenderType::Host,
            reserved: [0; 6],
        },
        payload,
    }
}
fn set(values: &[(u16, &[u8])]) -> Vec<u8> {
    let mut b = Vec::from((values.len() as u16).to_le_bytes());
    b.extend([0, 0]);
    for (k, v) in values {
        b.extend(k.to_le_bytes());
        b.extend((v.len() as u16).to_le_bytes());
        b.extend(*v);
    }
    b
}
#[test]
fn discovery_and_firmware_query() {
    let mut d = device();
    let ack = d.handle(&request(0, &[])).unwrap().unwrap();
    assert_eq!(ack.header.seq_num, u32::MAX);
    assert_eq!(ack.header.sender_type, SenderType::Lidar);
    assert_eq!(
        DiscoveryResponse::decode(&ack.payload).unwrap().device_type,
        9
    );
    assert_eq!(
        d.handle(&request(GET_PARAMETERS, &[1, 0, 0, 0, 0x10, 0x80]))
            .unwrap()
            .unwrap()
            .payload,
        [0, 1, 0, 0x10, 0x80, 1, 0, 1]
    );
}
#[test]
fn transactional_settings_and_stop() {
    let mut d = device();
    let b = set(&[(0x1a, &[1]), (0xffff, &[0])]);
    let ack = d.handle(&request(SET_PARAMETERS, &b)).unwrap().unwrap();
    assert_eq!(ack.payload, [0x20, 0xff, 0xff]);
    assert!(!d.sampling());
}
#[test]
fn settings_errors_and_pause() {
    let mut d = device();
    for (key, value, code) in [
        (0x1a, vec![1], 0),
        (0x1a, vec![2], 0),
        (0x1a, vec![9], 0),
        (0x1a, vec![255], 3),
        (0x1a, vec![], 0x23),
        (0, vec![2], 0x20),
        (0x1c, vec![1], 0x20),
        (0x8010, vec![0], 0x22),
        (0x1234, vec![0], 0x20),
    ] {
        let b = set(&[(key, &value)]);
        assert_eq!(
            d.handle(&request(SET_PARAMETERS, &b))
                .unwrap()
                .unwrap()
                .payload[0],
            code
        );
    }
    let b = set(&[(0x1a, &[1]), (0x1a, &[2])]);
    assert_eq!(
        d.handle(&request(SET_PARAMETERS, &b))
            .unwrap()
            .unwrap()
            .payload[0],
        0x24
    );
    assert_eq!(d.work_mode, 9);
    let mut p = request(0, &[]);
    p.header.cmd_type = CommandType::Ack;
    assert!(d.handle(&p).unwrap().is_none());
}
#[test]
fn endpoint_validation_and_malformed() {
    let mut d = device();
    let e = HostEndpoint {
        ip: [127, 0, 0, 1],
        destination_port: 60001,
        source_port: 56300,
    };
    let bytes = encode_fixed(&e).unwrap();
    let b = set(&[(6, &bytes)]);
    assert_eq!(
        d.handle(&request(SET_PARAMETERS, &b))
            .unwrap()
            .unwrap()
            .payload,
        [0, 0, 0]
    );
    assert_eq!(d.point_endpoint, e);
    for n in 0..b.len() {
        assert_ne!(
            d.handle(&request(SET_PARAMETERS, &b[..n]))
                .unwrap()
                .unwrap()
                .payload[0],
            0
        );
    }
    let b = set(&[(6, &[0; 8])]);
    assert_ne!(
        d.handle(&request(SET_PARAMETERS, &b))
            .unwrap()
            .unwrap()
            .payload[0],
        0
    );
    assert_eq!(d.point_endpoint, e);
    assert_eq!(
        d.handle(&request(GET_PARAMETERS, &[1, 0, 0, 0, 0xff, 0xff]))
            .unwrap()
            .unwrap()
            .payload,
        [0x20, 0, 0]
    );
}
