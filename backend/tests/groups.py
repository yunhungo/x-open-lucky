#!/usr/bin/env python3
# @created 2026-10-13
# @description 用隔离真实服务验证分组迁移、版本冲突、统计范围和备份兼容。
# @author yunhungo
import hashlib
import http.client
import json
import os
from pathlib import Path
import secrets
import sqlite3
import statistics
import subprocess
import tempfile
import time
from smoke import ROOT, free_port


def main():
    admin, proxy, tls = free_port(), free_port(), free_port()
    with tempfile.TemporaryDirectory(prefix='openlucky-groups-') as directory:
        root = Path(directory)
        data = root / 'data'
        data.mkdir()
        legacy = {'id': 'legacy-service', 'name': 'Existing service', 'enabled': True,
                  'domain': 'existing.example.test', 'target': f'http://127.0.0.1:{admin}',
                  'certificate_id': '', 'https_redirect': False, 'preserve_host': False, 'listener': 'default'}
        # 构造真实 v4 配置数据库，记录 SQLx 原始校验和；新进程必须执行 v5 迁移。
        with sqlite3.connect(data / 'openlucky.sqlite') as db:
            db.execute('CREATE TABLE _sqlx_migrations (version BIGINT PRIMARY KEY, description TEXT NOT NULL, installed_on TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP, success BOOLEAN NOT NULL, checksum BLOB NOT NULL, execution_time BIGINT NOT NULL)')
            for migration in sorted((ROOT / 'backend/migrations').glob('*.sql')):
                version, description = migration.stem.split('_', 1)
                if int(version) >= 5:
                    continue
                sql = migration.read_bytes()
                db.executescript(sql.decode())
                db.execute('INSERT INTO _sqlx_migrations(version,description,success,checksum,execution_time) VALUES(?,?,1,?,0)', (int(version), description.replace('_', ' '), hashlib.sha384(sql).digest()))
            db.execute('INSERT INTO resources(kind,id,data) VALUES(?,?,?)', ('web', legacy['id'], json.dumps(legacy)))
        env = dict(os.environ, OPENLUCKY_DATA=str(data), OPENLUCKY_ADMIN_ADDR=f'127.0.0.1:{admin}', OPENLUCKY_HTTP_ADDR=f'127.0.0.1:{proxy}', OPENLUCKY_HTTPS_ADDR=f'127.0.0.1:{tls}', OPENLUCKY_LISTENERS='[]')
        cookie = ''
        log = (root / 'server.log').open('w')
        process = subprocess.Popen([str(ROOT / 'target/debug/openlucky')], cwd=ROOT, env=env, stdout=log, stderr=log)

        def call(method, path, body=None, expected=200):
            nonlocal cookie
            connection = http.client.HTTPConnection('127.0.0.1', admin, timeout=15)
            connection.request(method, '/api/v1' + path, None if body is None else json.dumps(body), {'Content-Type': 'application/json', 'X-OpenLucky': '1', 'Cookie': cookie})
            response = connection.getresponse()
            raw = response.read()
            header = response.getheader('Set-Cookie')
            if header:
                cookie = header.split(';')[0]
            connection.close()
            assert response.status == expected, (method, path, response.status, raw[:300])
            return json.loads(raw)

        def services():
            return {s['id']: s for s in call('GET', '/resources/web')}

        def groups():
            return call('GET', '/groups')

        def versions(items):
            return [{'id': item['id'], 'revision': item['revision']} for item in items]

        def assign(group, items, expected=200):
            return call('PUT', '/groups/assign', {'group_id': group, 'services': versions(items)}, expected)

        try:
            for _ in range(100):
                assert process.poll() is None, (root / 'server.log').read_text()
                try:
                    call('GET', '/auth/status')
                    break
                except OSError:
                    time.sleep(.1)
            for endpoint in ['/groups', '/group-stats']:
                call('GET', endpoint, expected=401)
            call('POST', '/auth/setup', {'username': 'admin', 'password': secrets.token_urlsafe(24), 'setup_token': (data / 'setup-token').read_text()})
            original = services()[legacy['id']]
            assert original['group_id'] == '' and original['target'] == legacy['target']
            assert len(groups()) == 6
            with sqlite3.connect(data / 'openlucky.sqlite') as db:
                assert db.execute('SELECT COUNT(*) FROM _sqlx_migrations').fetchone()[0] == 5
            print('PASS groups v4 migration, existing fields and authenticated APIs')

            custom = call('POST', '/groups', {'name': '  Test Group  '})['id']
            assert next(g for g in groups() if g['id'] == custom)['name'] == 'Test Group'
            for name in ['', '   ', '未分组', 'test group', 'x' * 41, 'bad\nname']:
                call('POST', '/groups', {'name': name}, expected=400)
            current = next(g for g in groups() if g['id'] == custom)
            call('PUT', f'/groups/{custom}', {'name': 'Renamed', 'revision': current['revision']})
            call('PUT', f'/groups/{custom}', {'name': 'Stale', 'revision': current['revision']}, expected=409)
            call('DELETE', f'/groups/{custom}', {'revision': current['revision']}, expected=409)
            ordered = list(reversed(groups()))
            call('PUT', '/groups/order', {'groups': versions(ordered)})
            assert [g['id'] for g in groups()] == [g['id'] for g in ordered]
            call('PUT', '/groups/order', {'groups': versions(ordered)}, expected=409)
            call('PUT', '/groups/order', {'groups': versions(groups()[:-1])}, expected=409)
            print('PASS group names, revision checks, reorder roster and persistence')

            def create(name, group=''):
                return call('POST', '/resources/web', {**legacy, 'name': name, 'domain': f'{name.lower()}.example.test', 'group_id': group})['id']
            a, b, c = create('Alpha', custom), create('Beta'), create('Gamma')
            call('POST', '/resources/web', {**legacy, 'domain': 'invalid.example.test', 'group_id': 'missing'}, expected=404)
            before = services()
            stale = {**before[b], 'revision': before[b]['revision'] - 1}
            assign(custom, [before[c], stale], 409)
            assert services()[c] == before[c], 'Partially applied a failed batch'
            assign(custom, [before[b], before[c]])
            for resource in [b, c]:
                updated = services()[resource]
                assert updated['group_id'] == custom and updated['revision'] == before[resource]['revision'] + 1
                assert {k: v for k, v in updated.items() if k not in ('group_id', 'revision')} == {k: v for k, v in before[resource].items() if k not in ('group_id', 'revision')}
            call('PUT', f'/resources/web/{b}', {**before[b], 'target': 'http://127.0.0.1:19999'}, expected=409)
            assign(custom, [services()[a], services()[a]], 400)
            assign('missing', [services()[a]], 404)
            print('PASS atomic bulk assignment and preservation of upstream, listener and options')

            # 确定性小时桶验证加权平均；不依赖机器速度或外部访问。
            hour = int(time.time()) // 3600 * 3600
            rows = [(a, 2, 0, 1, 1, 20.), (b, 8, 2, 0, 0, 240.), (c, 0, 0, 0, 1, 0.), (legacy['id'], 1, 0, 0, 0, 5.), ('deleted-service', 3, 0, 0, 0, 30.), ('', 20, 4, 2, 2, 400.)]
            with sqlite3.connect(data / 'telemetry.sqlite') as db:
                db.executemany('INSERT INTO traffic(hour,service_id,requests,client_errors,server_errors,stream_errors,latency_ms) VALUES(?,?,?,?,?,?,?)', [(hour, *row) for row in rows])
                for index, sid in enumerate([a, b, a, legacy['id'], '', 'deleted-service']):
                    entry = {'time': int(time.time()), 'request_id': f'group-request-{index}', 'service_id': sid, 'host': 'fixture.example.test', 'listener': 'default', 'client_ip': '127.0.0.1', 'method': 'GET', 'path': '/fixture', 'status': 200, 'duration_ms': 5, 'error': '', 'phase': 'request', 'tls': False}
                    db.execute('INSERT INTO access_logs(time,service_id,status,failed,entry) VALUES(?,?,200,0,?)', (entry['time'], sid, json.dumps(entry)))
            stats = {g['id']: g for g in call('GET', '/group-stats')['groups']}
            assert stats[custom]['requests'] == 10 and stats[custom]['avg_duration_ms'] == 26
            assert stats[custom]['services'] == 3 and stats[custom]['stream_errors'] == 2
            assert stats['unassigned']['requests'] == 1 and stats['unassigned']['services'] == 1
            traffic = call('GET', f'/traffic?group_id={custom}')
            assert traffic['requests'] == 10 and traffic['avg_duration_ms'] == 26
            assert sum(bucket['requests'] for bucket in traffic['series']) == 10
            assert call('GET', '/traffic')['requests'] == 20
            assert call('GET', '/traffic?group_id=unassigned')['requests'] == 1
            assert call('GET', '/traffic?group_id=group-network')['requests'] == 0
            assert call('GET', f'/traffic?group_id=group-network&service_id={a}')['requests'] == 0
            for endpoint in ['/traffic', '/access-logs']:
                call('GET', endpoint + '?group_id=missing', expected=404)
                call('GET', endpoint + '?group_id=' + 'x' * 65, expected=400)
            call('GET', '/group-stats?hours=2', expected=400)
            first = call('GET', f'/access-logs?group_id={custom}&limit=2')
            second = call('GET', f'/access-logs?group_id={custom}&limit=2&before={first["next_cursor"]}')
            assert len(first['items']) == 2 and len(second['items']) == 1
            assert not ({e['id'] for e in first['items']} & {e['id'] for e in second['items']})
            assert all(e['service_id'] in [a, b, c] for e in first['items'] + second['items'])
            assert len(call('GET', '/access-logs?group_id=unassigned')['items']) == 1
            assert not call('GET', f'/access-logs?group_id=group-network&service_id={a}')['items']
            assign('', [services()[a]])
            assert call('GET', f'/traffic?group_id={custom}')['requests'] == 8
            assert call('GET', '/traffic?group_id=unassigned')['requests'] == 3
            print('PASS weighted group statistics, current membership, unknown routes and scoped log pagination')

            # 新备份保留顺序及关联，但使用新 ID；旧版本密文夹具无分组字段。
            password = secrets.token_urlsafe(24)
            backup = call('POST', '/backup/export', {'password': password})['backup']
            safe_services, safe_groups = services(), groups()
            with sqlite3.connect(data / 'openlucky.sqlite') as db:
                db.execute("UPDATE resources SET data=json_set(data,'$.group_id','missing-group') WHERE id=?", (b,))
            invalid = call('POST', '/backup/export', {'password': password})['backup']
            with sqlite3.connect(data / 'openlucky.sqlite') as db:
                db.execute("UPDATE resources SET data=json_set(data,'$.group_id',?) WHERE id=?", (custom, b))
            call('POST', '/backup/restore', {'password': password, 'backup': invalid}, expected=400)
            assert services() == safe_services and groups() == safe_groups, 'Invalid backup changed configuration'
            old_order = [g['name'] for g in groups()]
            old_services = services()
            call('POST', '/backup/restore', {'password': password, 'backup': backup})
            new_groups = groups()
            new_custom = next(g for g in new_groups if g['name'] == 'Renamed')['id']
            assert [g['name'] for g in new_groups] == old_order and new_custom != custom
            restored = services()
            assert not set(restored).intersection(old_services)
            assert all(s['group_id'] == new_custom for s in restored.values() if s['name'] in ['Beta', 'Gamma'])
            assert all(s['target'] == legacy['target'] for s in restored.values())
            call('DELETE', f'/groups/{new_custom}', {'revision': next(g['revision'] for g in groups() if g['id'] == new_custom)})
            for sid, service in services().items():
                assert service['group_id'] == ''
                assert {k: v for k, v in service.items() if k not in ('group_id', 'revision')} == {k: v for k, v in restored[sid].items() if k not in ('group_id', 'revision')}
            assert len(services()) == 4
            saved_groups = groups()
            fixture = (ROOT / 'backend/tests/fixtures/backup-v1.json').read_text()
            call('POST', '/backup/restore', {'password': 'fixture-only-not-a-real-password', 'backup': fixture})
            assert groups() == saved_groups
            migrated = list(services().values())
            assert len(migrated) == 1 and migrated[0]['name'] == 'Legacy service' and migrated[0]['group_id'] == ''
            print('PASS v2 backup ID remapping, safe group deletion and real v1 encrypted backup compatibility')

            # 42 服务 × 720 小时模拟完整保留期，记录管理查询开销，不把本机耗时当 NAS 指标。
            definitions = groups()
            load_ids = [create(f'Load-{i}', definitions[i % len(definitions)]['id']) for i in range(42)]
            with sqlite3.connect(data / 'telemetry.sqlite') as db:
                db.executemany('INSERT INTO traffic(hour,service_id,requests,client_errors,server_errors,stream_errors,latency_ms) VALUES(?,?,10,0,0,0,500)', [(hour - offset * 3600, sid) for sid in load_ids for offset in range(720)])
            timings = []
            for _ in range(10):
                begin = time.perf_counter()
                summary = call('GET', '/group-stats?hours=720')
                timings.append((time.perf_counter() - begin) * 1000)
                assert sum(g['requests'] for g in summary['groups']) == 42 * 720 * 10
                assert all(g['avg_duration_ms'] == 50 for g in summary['groups'] if g['requests'])
            print(f'PASS batched group summary: 42 services / 30240 hour buckets; local debug median={statistics.median(timings):.1f} ms max={max(timings):.1f} ms')
        finally:
            process.terminate()
            try:
                process.wait(timeout=40)
            except subprocess.TimeoutExpired:
                process.kill(); process.wait()
            log.close()


if __name__ == '__main__':
    main()
