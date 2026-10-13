#!/usr/bin/env python3
# @created 2026-10-13
# @description 回归验证多个 TCP/QUIC 监听器同时响应 SIGTERM 时正常退出。
# @author yunhungo
import http.client
import os
from pathlib import Path
import subprocess
import tempfile
import time
from smoke import free_port

ROOT = Path(__file__).resolve().parents[2]


def main():
    admin, plain, tls, extra = [free_port() for _ in range(4)]
    with tempfile.TemporaryDirectory(prefix='openlucky-shutdown-') as folder:
        root = Path(folder)
        env = dict(os.environ, OPENLUCKY_DATA=str(root / 'data'),
                   OPENLUCKY_ADMIN_ADDR=f'127.0.0.1:{admin}',
                   OPENLUCKY_HTTP_ADDR=f'127.0.0.1:{plain}',
                   OPENLUCKY_HTTPS_ADDR=f'127.0.0.1:{tls}',
                   OPENLUCKY_HTTP3_ADDR='', OPENLUCKY_PUBLIC_HTTP3_PORT='',
                   OPENLUCKY_LISTENERS=f'[{{"id":"extra","addr":"127.0.0.1:{extra}","https_port":{extra}}}]')
        for iteration in range(20):
            with (root / 'server.log').open('w') as log:
                process = subprocess.Popen([str(ROOT / 'target/debug/openlucky')], env=env, stdout=log, stderr=log)
                try:
                    for _ in range(200):
                        if process.poll() is not None:
                            raise AssertionError((root / 'server.log').read_text())
                        try:
                            conn = http.client.HTTPConnection('127.0.0.1', admin, timeout=1)
                            conn.request('GET', '/healthz')
                            response = conn.getresponse(); response.read(); conn.close()
                            if response.status == 200: break
                        except OSError: pass
                        time.sleep(.025)
                    else: raise AssertionError('Server readiness timeout')
                    process.terminate()
                    process.wait(timeout=40)
                    assert process.returncode == 0, (iteration, process.returncode, (root / 'server.log').read_text())
                finally:
                    if process.poll() is None:
                        process.kill(); process.wait()
    print('PASS 20 consecutive graceful restarts with multiple TCP/QUIC listeners', flush=True)


if __name__ == '__main__': main()
