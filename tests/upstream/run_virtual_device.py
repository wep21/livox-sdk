#!/usr/bin/env python3
"""Run an already-built SDK2 public-API client against the Rust virtual device.
No third-party Python packages. SDK client needs two distinct local/remote IPs;
when both processes are local, both IPs must be bindable on this machine.
"""
import argparse
import json
from pathlib import Path
import subprocess
import tempfile
import time

p = argparse.ArgumentParser()
p.add_argument('--sdk-client', type=Path, required=True)
p.add_argument('--simulator', type=Path, default=Path('target/release/livox-virtual-device'))
p.add_argument('--capture', type=Path, default=Path('fixtures/official-mid360-first-frame.lvx2'))
p.add_argument('--host', required=True)
p.add_argument('--device', default='127.0.0.1')
a = p.parse_args()
if a.host == a.device:
    p.error('SDK ignores packets from its own host IP; use distinct bindable addresses')
config = {'lidar_log_enable': False, 'MID360': {
    'lidar_net_info': {'cmd_data_port': 56100, 'push_msg_port': 56200,
                       'point_data_port': 56300, 'imu_data_port': 56400, 'log_data_port': 56500},
    'host_net_info': [{'lidar_ip': [a.device], 'host_ip': a.host, 'cmd_data_port': 56101,
                       'push_msg_port': 56201, 'point_data_port': 56301,
                       'imu_data_port': 56401, 'log_data_port': 56501}]}}
with tempfile.TemporaryDirectory(prefix='livox-interop-') as tmp:
    path = Path(tmp) / 'sdk.json'
    path.write_text(json.dumps(config, indent=2))
    simulator = subprocess.Popen([str(a.simulator.resolve()), 'replay', str(a.capture.resolve()),
        '--bind', a.device, '--host', a.host, '--loop', '--run-for', '25'], cwd=tmp)
    try:
        time.sleep(0.25)
        if simulator.poll() is not None:
            raise SystemExit('simulator exited before SDK start')
        result = subprocess.run([str(a.sdk_client.resolve()), str(path)], cwd=tmp, timeout=25)
        raise SystemExit(result.returncode)
    finally:
        simulator.terminate()
        try:
            simulator.wait(timeout=3)
        except subprocess.TimeoutExpired:
            simulator.kill()
            simulator.wait()
