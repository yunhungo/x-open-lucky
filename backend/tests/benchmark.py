#!/usr/bin/env python3
# @created 2026-10-13
# @description 在相同回环负载下比较两个 release 构建并采样代理内存与 CPU。
# @author yunhungo
import argparse
import http.client
import json
import os
from pathlib import Path
import secrets
import statistics
import subprocess
import tempfile
import threading
import time
from smoke import free_port
from quic_latency import QuicLatency

ROOT = Path(__file__).resolve().parents[2]


def sample(pid):
    row = subprocess.check_output(['ps', '-p', str(pid), '-o', 'rss=,time='], text=True).split()
    clock = row[1].split(':')
    seconds = float(clock[-1]) + int(clock[-2]) * 60
    if len(clock) == 3: seconds += int(clock[0]) * 3600
    return int(row[0]), seconds


def run(binary, name, upstream, rounds, requests, duration, concurrency, baseline_http3, quic_rtt_ms, large_only):
    ports = [free_port() for _ in range(3)]
    with tempfile.TemporaryDirectory(prefix='openlucky-bench-') as folder:
        root = Path(folder)
        env = dict(os.environ, OPENLUCKY_DATA=str(root / 'data'), OPENLUCKY_ADMIN_ADDR=f'127.0.0.1:{ports[0]}', OPENLUCKY_HTTP_ADDR=f'127.0.0.1:{ports[1]}', OPENLUCKY_HTTPS_ADDR=f'127.0.0.1:{ports[2]}', OPENLUCKY_PUBLIC_HTTPS_PORT=str(ports[2]), OPENLUCKY_HTTP3_ADDR='', OPENLUCKY_PUBLIC_HTTP3_PORT='', OPENLUCKY_LISTENERS='[]')
        log = (root / 'server.log').open('w')
        process = subprocess.Popen([str(binary)], cwd=ROOT, env=env, stdout=log, stderr=log)
        cookie = ''
        def api(method, path, value=None):
            nonlocal cookie
            conn = http.client.HTTPConnection('127.0.0.1', ports[0], timeout=20)
            conn.request(method, '/api/v1' + path, None if value is None else json.dumps(value), {'Content-Type': 'application/json', 'X-OpenLucky': '1', 'Cookie': cookie})
            response = conn.getresponse(); data = response.read(); conn.close()
            if response.getheader('Set-Cookie'): cookie = response.getheader('Set-Cookie').split(';')[0]
            assert response.status == 200, (response.status, data[:200])
            return json.loads(data)
        try:
            for _ in range(100):
                if process.poll() is not None: raise AssertionError((root / 'server.log').read_text())
                if (root / 'data/setup-token').exists():
                    try:
                        api('GET', '/auth/status'); break
                    except OSError: pass
                time.sleep(.1)
            api('POST', '/auth/setup', {'username': 'admin', 'password': secrets.token_urlsafe(24), 'setup_token': (root / 'data/setup-token').read_text()})
            subprocess.run(['openssl', 'req', '-x509', '-newkey', 'rsa:2048', '-nodes', '-keyout', str(root / 'key.pem'), '-out', str(root / 'cert.pem'), '-days', '2', '-subj', '/CN=bench.example.test', '-addext', 'subjectAltName=DNS:bench.example.test', '-addext', 'basicConstraints=critical,CA:FALSE'], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=True)
            cert = api('POST', '/resources/certificates', {'name': 'Bench TLS', 'domains': ['bench.example.test'], 'email': '', 'credential_id': '', 'staging': False, 'auto_renew': False, 'imported': True})['id']
            api('POST', f'/certificates/{cert}/import', {'cert_pem': (root / 'cert.pem').read_text(), 'key_pem': (root / 'key.pem').read_text()})
            rule = {'name': 'Bench', 'domain': 'bench.example.test', 'target': f'http://127.0.0.1:{upstream}', 'enabled': True, 'certificate_id': cert, 'https_redirect': False, 'preserve_host': False}
            wid = api('POST', '/resources/web', rule)['id']
            for i in range(41): api('POST', '/resources/web', {**rule, 'name': f'Route {i}', 'domain': f'route-{i}.example.test', 'certificate_id': ''})
            def configure(dynamic):
                current = next(v for v in api('GET', '/resources/web') if v['id'] == wid)
                options = {**current.get('options', {}), 'http3': True, 'request_headers': {'X-Host': '{{ request.host }}', 'X-Origin': '{{ request.scheme | upper }}', 'X-Client': '{{ client.ip }}'} if dynamic else {}, 'response_headers': {'X-Status': '{{ response.status }}'} if dynamic else {}}
                api('PUT', '/resources/web/' + wid, {**current, 'options': options})
            modern = name == 'candidate' or baseline_http3
            if modern: configure(False)
            expected_requests = 0
            def load(protocol, count, path='/bench', parallel=concurrency, seconds=0):
                nonlocal expected_requests
                h3 = protocol == 'http3'
                options = {'address': f'127.0.0.1:{ports[1] if protocol == "http1" else ports[2]}', 'requests': count, 'concurrency': parallel, 'path': path, 'duration_secs': seconds}
                if h3: options.update(server_name='bench.example.test', ca=str(root / 'cert.pem'))
                else: options.update(domain='bench.example.test', version=protocol, ca=None if protocol == 'http1' else str(root / 'cert.pem'))
                binary_name = 'h3_probe' if h3 else 'proxy_load'
                relay = QuicLatency(ports[2], quic_rtt_ms) if h3 and quic_rtt_ms else None
                if relay:
                    options['address'] = f'127.0.0.1:{relay.port}'
                try:
                    result = subprocess.run([str(ROOT / 'target/release/examples' / binary_name)], input=json.dumps(options), text=True, capture_output=True, timeout=seconds + 90)
                finally:
                    if relay: relay.close()
                assert result.returncode == 0, result.stderr + '\n' + (root / 'server.log').read_text()[-8000:]
                value = json.loads(result.stdout)
                assert value['failures'] == 0, (value, api('GET', '/access-logs?errors=true&limit=3'))
                expected_requests += value['requests']
                value.pop('response', None)
                if relay:
                    value.update(simulated_rtt_ms=quic_rtt_ms, relay_drops=relay.dropped)
                    assert relay.dropped == 0, value
                return value
            results = []
            for protocol in ['http1', 'http2'] + (['http3'] if modern else []):
                if large_only: continue
                load(protocol, 1000)
                for iteration in range(rounds):
                    stop = threading.Event(); rss = []
                    before = sample(process.pid)
                    def monitor():
                        while not stop.wait(.1): rss.append(sample(process.pid)[0])
                    monitor_thread = threading.Thread(target=monitor); monitor_thread.start()
                    try: value = load(protocol, requests, seconds=duration)
                    finally: stop.set(); monitor_thread.join()
                    after = sample(process.pid)
                    value.update(protocol=protocol, round=iteration + 1, peak_rss_mib=max([before[0], after[0], *rss])/1024, cpu_seconds=round(after[1]-before[1], 3))
                    results.append(value)
                    print(json.dumps({'build': name, **value}), flush=True)
            if modern:
                if not large_only:
                    configure(True)
                    for iteration in range(rounds):
                        value = load('http1', requests, seconds=duration)
                        value.update(protocol='http1_templates', round=iteration + 1)
                        results.append(value); print(json.dumps({'build': name, **value}), flush=True)
                configure(False)
                for protocol in ['http1', 'http3']:
                    before = sample(process.pid); rss = []; stop = threading.Event()
                    def monitor_large():
                        while not stop.wait(.05): rss.append(sample(process.pid)[0])
                    monitor_thread = threading.Thread(target=monitor_large); monitor_thread.start()
                    try: value = load(protocol, 16, '/large', 8)
                    finally: stop.set(); monitor_thread.join()
                    time.sleep(2)
                    after = sample(process.pid)
                    value.update(protocol=protocol + '_16MiB', peak_rss_mib=max([before[0], after[0], *rss])/1024, after_rss_mib=after[0]/1024)
                    results.append(value); print(json.dumps({'build': name, **value}), flush=True)
            for _ in range(120):
                traffic = api('GET', '/traffic')
                if traffic['requests'] == expected_requests: break
                time.sleep(.1)
            assert traffic['requests'] == expected_requests, (traffic['requests'], expected_requests)
            return {'build': name, 'binary': str(binary), 'route_count': 42, 'results': results, 'counted_requests': traffic['requests'], 'telemetry': api('GET', '/telemetry')}
        finally:
            process.terminate()
            try: process.wait(timeout=40)
            except subprocess.TimeoutExpired: process.kill(); process.wait()
            log.close()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--baseline', type=Path, required=True)
    parser.add_argument('--candidate', type=Path, default=ROOT / 'target/release/openlucky')
    parser.add_argument('--rounds', type=int, default=3)
    parser.add_argument('--requests', type=int, default=30000)
    parser.add_argument('--candidate-only', action='store_true')
    parser.add_argument('--duration', type=int, default=0, help='每轮持续秒数（1–120），0 使用请求数')
    parser.add_argument('--concurrency', type=int, default=32)
    parser.add_argument('--baseline-http3', action='store_true', help='基线也支持 HTTP/3 与动态头时进行相同场景对照')
    parser.add_argument('--quic-rtt-ms', type=int, default=0, help='通过有界 UDP 回环中继模拟 QUIC RTT（0–100 ms）')
    parser.add_argument('--large-only', action='store_true', help='只运行大文件对照')
    parser.add_argument('--output', type=Path, default=ROOT / '.myself/qa/web-performance.json')
    args = parser.parse_args()
    if not 0 <= args.duration <= 120 or not 1 <= args.concurrency <= 256 or args.rounds < 1 or not 0 <= args.quic_rtt_ms <= 100:
        parser.error('Invalid duration, concurrency or rounds')
    upstream_port = free_port()
    upstream = subprocess.Popen([str(ROOT / 'target/release/examples/protocol_upstream'), f'127.0.0.1:{upstream_port}'], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    try:
        output = []
        args.output.parent.mkdir(parents=True, exist_ok=True)
        builds = [] if args.candidate_only else [(args.baseline.resolve(), 'baseline')]
        for binary, name in builds + [(args.candidate.resolve(), 'candidate')]:
            output.append(run(binary, name, upstream_port, args.rounds, args.requests, args.duration, args.concurrency, args.baseline_http3, args.quic_rtt_ms, args.large_only))
            args.output.write_text(json.dumps(output, indent=2))
        for build in output:
            for protocol in dict.fromkeys(v['protocol'] for v in build['results']):
                rows = [v for v in build['results'] if v['protocol'] == protocol]
                print(json.dumps({'summary': build['build'], 'protocol': protocol, 'median_rps': round(statistics.median(v['rps'] for v in rows)), 'median_p95_ms': round(statistics.median(v['p95_ms'] for v in rows), 3)}), flush=True)
    finally:
        upstream.terminate(); upstream.wait(timeout=5)


if __name__ == '__main__': main()
