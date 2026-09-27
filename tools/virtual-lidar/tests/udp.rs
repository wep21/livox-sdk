use livox_protocol::{
    CommandHeader, CommandPacket, CommandPacketOwned, CommandType, DataPacket, SenderType,
    control::*,
};
use std::{
    io::{BufRead, BufReader},
    net::UdpSocket,
    process::{Child, Command, Stdio},
    time::Duration,
};
struct Process(Child);
impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn start(receiver: u16, extra: &[&str]) -> (Process, u16, u16, u16) {
    let path = format!(
        "{}/../../fixtures/official-mid360-first-frame.lvx2",
        env!("CARGO_MANIFEST_DIR")
    );
    let mut c = Command::new(env!("CARGO_BIN_EXE_livox-virtual-device"));
    c.args([
        "replay",
        &path,
        "--discovery-port",
        "0",
        "--command-port",
        "0",
        "--point-source-port",
        "0",
        "--point-port",
        &receiver.to_string(),
        "--no-announce",
        "--run-for",
        "5",
    ]);
    c.args(extra);
    c.stdout(Stdio::piped());
    c.stderr(Stdio::inherit());
    let mut p = Process(c.spawn().unwrap());
    let out = p.0.stdout.take().unwrap();
    let mut line = String::new();
    let mut reader = BufReader::new(out);
    reader.read_line(&mut line).unwrap();
    p.0.stdout = Some(reader.into_inner());
    assert!(line.starts_with("ready:"), "{line}");
    let port = |key: &str| {
        line.split_whitespace()
            .find_map(|v| v.strip_prefix(key))
            .unwrap()
            .parse::<u16>()
            .unwrap()
    };
    let ports = (port("discovery="), port("command="), port("point_source="));
    (p, ports.0, ports.1, ports.2)
}
fn request(s: &UdpSocket, port: u16, cmd: u16, payload: Vec<u8>) -> Vec<u8> {
    let p = CommandPacketOwned {
        header: CommandHeader {
            seq_num: 0x12345678,
            cmd_id: cmd,
            cmd_type: CommandType::Request,
            sender_type: SenderType::Host,
            reserved: [0; 6],
        },
        payload,
    };
    let mut b = Vec::new();
    p.encode(&mut b).unwrap();
    s.send_to(&b, ("127.0.0.1", port)).unwrap();
    let mut buf = [0; 1400];
    let (n, peer) = s.recv_from(&mut buf).unwrap();
    assert_eq!(peer.port(), port);
    let ack = CommandPacket::decode(&buf[..n]).unwrap();
    assert_eq!(ack.header.seq_num, 0x12345678);
    assert_eq!(ack.header.cmd_type, CommandType::Ack);
    ack.payload.to_vec()
}
fn set(key: u16, value: &[u8]) -> Vec<u8> {
    let mut b = vec![1, 0, 0, 0];
    b.extend(key.to_le_bytes());
    b.extend((value.len() as u16).to_le_bytes());
    b.extend(value);
    b
}
fn socket() -> UdpSocket {
    let s = UdpSocket::bind("127.0.0.1:0").unwrap();
    s.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
    s
}
#[test]
fn discovery_configure_start_pause_resume_and_crc() {
    let points = socket();
    points
        .set_read_timeout(Some(Duration::from_millis(80)))
        .unwrap();
    let command = socket();
    let (_p, discovery, control, source) = start(
        points.local_addr().unwrap().port(),
        &["--loop", "--speed", "0.05"],
    );
    let mut buf = [0; 2000];
    assert!(points.recv_from(&mut buf).is_err());
    let ack = request(&command, discovery, DISCOVERY, vec![]);
    let discovery = DiscoveryResponse::decode(&ack).unwrap();
    assert_eq!(discovery.command_port, control);
    assert_eq!(discovery.device_type, 9);
    assert_eq!(
        request(
            &command,
            control,
            GET_PARAMETERS,
            vec![1, 0, 0, 0, 0x10, 0x80]
        ),
        [0, 1, 0, 0x10, 0x80, 1, 0, 1]
    );
    let e = HostEndpoint {
        ip: [127, 0, 0, 1],
        destination_port: points.local_addr().unwrap().port(),
        source_port: source,
    };
    assert_eq!(
        request(
            &command,
            control,
            SET_PARAMETERS,
            set(6, &encode_fixed(&e).unwrap())
        ),
        [0, 0, 0]
    );
    // A malformed frame cannot alter state, and is followed by a valid command.
    command
        .send_to(&[0xaa; 30], ("127.0.0.1", control))
        .unwrap();
    assert_eq!(
        request(&command, control, SET_PARAMETERS, set(0x1a, &[1])),
        [0, 0, 0]
    );
    let (n, peer) = points.recv_from(&mut buf).unwrap();
    assert_eq!(peer.port(), source);
    let first = DataPacket::decode(&buf[..n]).unwrap();
    assert_eq!(first.header.timestamp, 711620799460);
    assert_eq!(first.header.dot_num, 96);
    assert_eq!(
        &buf[36..n],
        &include_bytes!("../../../fixtures/official-mid360-first-frame.lvx2")[143..1487]
    );
    assert_eq!(
        request(&command, control, SET_PARAMETERS, set(0x1a, &[2])),
        [0, 0, 0]
    );
    while points.recv_from(&mut buf).is_ok() {} // Drain datagrams already in flight before ACK.
    assert!(points.recv_from(&mut buf).is_err());
    assert_eq!(
        request(&command, control, SET_PARAMETERS, set(0x1a, &[1])),
        [0, 0, 0]
    );
    assert!(points.recv_from(&mut buf).is_ok());
}
#[test]
fn direct_replay_loop_timestamps_and_packet_limit() {
    let points = socket();
    let (mut p, _, _, _) = start(
        points.local_addr().unwrap().port(),
        &["--start", "--loop", "--speed", "5", "--max-packets", "105"],
    );
    let mut buf = [0; 2000];
    let mut first = 0;
    let mut last = 0;
    for i in 0..105 {
        let (n, _) = points.recv_from(&mut buf).unwrap();
        let d = DataPacket::decode(&buf[..n]).unwrap();
        if i == 0 {
            first = d.header.timestamp;
        }
        if i > 0 {
            assert!(d.header.timestamp >= last);
        }
        last = d.header.timestamp;
    }
    assert_eq!(last, first + 49_440_000 + 50_000_000);
    assert!(p.0.wait().unwrap().success());
}
