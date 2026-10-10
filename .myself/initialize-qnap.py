#!/usr/bin/env python3
# @created 2026-10-11
# @description 经 SSH 一次性初始化管理员并把初始凭据保存在本机受限文件。
# @author yunhungo
import json
import os
from pathlib import Path
import re
import secrets
import subprocess
import sys


def main():
    host, docker = sys.argv[1:3]
    if not re.fullmatch(r'[a-zA-Z0-9@._:-]+', host) or not re.fullmatch(r'/[a-zA-Z0-9_./-]+', docker):
        raise SystemExit('Invalid SSH arguments')
    ssh = ['ssh', '-o', 'BatchMode=yes', '-o', 'ConnectTimeout=10', host]

    def remote(command, data=None):
        result = subprocess.run(ssh + [command], input=data, capture_output=True, text=True, timeout=45)
        if result.returncode:
            # 不打印任何可能包含初始化密钥或密码的响应。
            raise RuntimeError('管理员初始化请求失败，请保留本机凭据文件后重试。')
        return result.stdout

    address = remote(f"{docker} exec openlucky /bin/sh -c 'printf %s \"$OPENLUCKY_ADMIN_ADDR\"'").strip()
    if not re.fullmatch(r'[a-zA-Z0-9.:[\]-]+', address):
        raise RuntimeError('无效的管理监听地址')
    base = f'http://{address}/api/v1'
    status = json.loads(remote(f"{docker} exec openlucky curl --fail --silent --max-time 15 '{base}/auth/status'"))
    if status['initialized']:
        print('管理员已初始化，保留现有账户。')
        return
    token = remote(f'{docker} exec openlucky cat /data/setup-token').strip()
    access_file = Path(__file__).resolve().parent / 'qnap-access.json'
    username = os.environ.get('OPENLUCKY_INITIAL_ADMIN', 'yunhungo')
    if not re.fullmatch(r'[a-zA-Z0-9_-]{3,50}', username):
        raise RuntimeError('管理员用户名无效')
    if access_file.exists():
        details = json.loads(access_file.read_text())
        username, password = details['username'], details['password']
    else:
        password = secrets.token_urlsafe(32)
        details = {'url': f'http://{address}', 'username': username, 'password': password}
        fd = os.open(access_file, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
        with os.fdopen(fd, 'w') as file:
            json.dump(details, file, ensure_ascii=False, indent=2)
            file.flush(); os.fsync(file.fileno())
    payload = json.dumps({'username': username, 'password': password, 'setup_token': token})
    response = json.loads(remote(f"{docker} exec -i openlucky curl --fail --silent --max-time 30 -X POST -H 'Content-Type: application/json' -H 'X-OpenLucky: 1' --data-binary @- '{base}/auth/setup'", payload))
    if not response.get('ok'):
        raise RuntimeError('管理员初始化未成功')
    print(f'管理员已初始化，初始凭据：{access_file}')


if __name__ == '__main__':
    main()
