use std::{
    io::{BufRead, BufReader, Read, Write},
    net::{TcpStream, UdpSocket},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

struct Viewer {
    child: Child,
    address: String,
    source: String,
}
impl Drop for Viewer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
impl Viewer {
    fn start(args: &[&str]) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_livox-viewer"))
            .args(args)
            .args(["--http-port", "0"])
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        let mut stdout = BufReader::new(child.stdout.take().unwrap());
        let mut line = String::new();
        stdout.read_line(&mut line).unwrap();
        let address = line
            .trim()
            .strip_prefix("viewer: http://")
            .expect(&line)
            .to_owned();
        line.clear();
        stdout.read_line(&mut line).unwrap();
        let source = line
            .trim()
            .strip_prefix("source: ")
            .expect(&line)
            .to_owned();
        child.stdout = Some(stdout.into_inner());
        Self {
            child,
            address,
            source,
        }
    }
    fn request(&self, method: &str, path: &str, headers: &str) -> (String, Vec<u8>) {
        let mut s = TcpStream::connect(&self.address).unwrap();
        s.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
        write!(
            s,
            "{method} {path} HTTP/1.1\r\nHost: {}\r\nContent-Length: 0\r\n{headers}\r\n",
            self.address
        )
        .unwrap();
        // Read the framed response, not to EOF: rejecting oversized requests can
        // close a socket with unread request bytes (TCP reset on some platforms).
        let mut reader = BufReader::new(s);
        let mut header = String::new();
        loop {
            let mut line = String::new();
            assert!(reader.read_line(&mut line).unwrap() > 0);
            header.push_str(&line);
            if line == "\r\n" {
                break;
            }
        }
        let length: usize = header
            .lines()
            .find_map(|l| l.strip_prefix("Content-Length: "))
            .unwrap()
            .parse()
            .unwrap();
        let mut body = vec![0; length];
        reader.read_exact(&mut body).unwrap();
        (header, body)
    }
    fn status(&self, method: &str, path: &str) -> serde_json::Value {
        let (h, b) = self.request(method, path, "");
        assert!(h.starts_with("HTTP/1.1 200"), "{h}");
        let n = u32::from_le_bytes(b[..4].try_into().unwrap()) as usize;
        let s: serde_json::Value = serde_json::from_slice(&b[4..4 + n]).unwrap();
        assert_eq!(b.len() - 4 - n, s["points"].as_u64().unwrap() as usize * 16);
        s
    }
}

#[test]
fn http_assets_playback_limits_and_request_boundaries() {
    let path = format!(
        "{}/../../fixtures/official-mid360-first-frame.lvx2",
        env!("CARGO_MANIFEST_DIR")
    );
    let v = Viewer::start(&["file", &path, "--max-points", "100"]);
    for path in ["/", "/viewer.js", "/style.css", "/api/info"] {
        let (h, b) = v.request("GET", path, "");
        assert!(h.starts_with("HTTP/1.1 200"));
        assert!(!b.is_empty());
    }
    assert_eq!(v.status("GET", "/api/cloud")["packets"], 0);
    let (h, _) = v.request("POST", "/api/next", "Origin: http://untrusted.invalid\r\n");
    assert!(h.starts_with("HTTP/1.1 403"));
    assert_eq!(v.status("GET", "/api/cloud")["packets"], 0);
    assert!(
        v.request("GET", "/api/next", "")
            .0
            .starts_with("HTTP/1.1 404")
    );
    let first = v.status("POST", "/api/next");
    assert_eq!(first["points"], 100);
    assert_eq!(first["packets"], 104);
    assert_eq!(v.status("POST", "/api/next")["ended"], true);
    assert_eq!(v.status("POST", "/api/reset"), first);
    let mut oversized = TcpStream::connect(&v.address).unwrap();
    oversized
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    write!(
        oversized,
        "GET / HTTP/1.1\r\nHost: {}\r\nX-Fill: {}\r\n\r\n",
        v.address,
        "a".repeat(8200)
    )
    .unwrap();
    let mut rejected = vec![];
    match oversized.read_to_end(&mut rejected) {
        Ok(_) => assert!(rejected.starts_with(b"HTTP/1.1 400")),
        Err(e) => assert_eq!(e.kind(), std::io::ErrorKind::ConnectionReset),
    }
    assert_eq!(v.status("GET", "/api/cloud"), first);
}

#[test]
fn actual_udp_receiver_validates_and_expires_points() {
    let v = Viewer::start(&["udp", "--bind", "127.0.0.1:0", "--window-ms", "250"]);
    let addr = v.source.strip_prefix("udp ").unwrap();
    let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
    socket.send_to(&[0; 5], addr).unwrap();
    socket
        .send_to(include_bytes!("../../../fixtures/sdk2-high.bin"), addr)
        .unwrap();
    let until = Instant::now() + Duration::from_secs(2);
    loop {
        let s = v.status("GET", "/api/cloud");
        if s["packets"] == 1 {
            assert!(s["points"].as_u64().unwrap() > 0);
            assert_eq!(s["invalid"], 1);
            break;
        }
        assert!(Instant::now() < until, "UDP did not arrive");
        std::thread::sleep(Duration::from_millis(10));
    }
    std::thread::sleep(Duration::from_millis(350));
    assert_eq!(v.status("GET", "/api/cloud")["points"], 0);
}
