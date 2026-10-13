#!/usr/bin/env python3
# @created 2026-10-11
# @description 用隔离 Docker 夹具验证部署成功、回滚和快照失败恢复。
# @author yunhungo
import json
import os
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[2]
SHA = 'a' * 40
OLD_SHA = 'b' * 40
MOCK = r'''#!/usr/bin/env python3
import json,os,sys,tarfile
from pathlib import Path
args=sys.argv[1:]; root=Path(os.environ['FAKE_ROOT']); file=root/'docker-state.json'
state=json.loads(file.read_text()); state['calls'].append(args)
def save(): file.write_text(json.dumps(state))
def output(value): print(value);save();sys.exit(0)
if args[:2]==['image','inspect']:
    tag=args[2]
    if 'source' in args[-1]: output('foreign' if tag=='openlucky:foreign' else 'https://github.com/yunhungo/x-open-lucky')
    else: output(os.environ['FAKE_SHA'] if tag==os.environ['FAKE_IMAGE'] else os.environ['FAKE_OLD_SHA'])
if args[0]=='inspect':
    if '.Config.Env' in args[-1]: output('OPENLUCKY_ADMIN_ADDR=192.168.1.85:24050')
    output(state['image'])
if args[0]=='stop': state['running']=False;save();sys.exit(0)
if args[0]=='run':
    if '-czf' in args:
        if os.environ['FAKE_MODE']=='snapshot-failure': save();sys.exit(1)
        with tarfile.open(fileobj=sys.stdout.buffer,mode='w:gz') as archive: archive.add(root/'data',arcname='data')
    elif '-xzf' in args:
        with tarfile.open(fileobj=sys.stdin.buffer,mode='r:gz') as archive: archive.extractall(root,filter='data')
    save();sys.exit(0)
if args[0]=='compose':
    state['admins'].append(os.environ.get('OPENLUCKY_ADMIN_ADDR'))
    if 'down' in args: state['running']=False
    if 'up' in args:
        state['image']=os.environ['OPENLUCKY_IMAGE'];state['running']=True
        if state['image']==os.environ['FAKE_IMAGE']:
            (root/'data'/'marker').write_text('new-version')
            if os.environ['FAKE_MODE']=='start-failure': save();sys.exit(1)
    save();sys.exit(0)
if args[0]=='exec': output(json.dumps({'ok':True,'build_sha':os.environ['FAKE_SHA'] if state['image']==os.environ['FAKE_IMAGE'] else os.environ['FAKE_OLD_SHA']},separators=(',',':')))
if args[:2]==['image','ls']:
    if any('dangling' in a for a in args): output('deadbeef')
    output('\n'.join([os.environ['FAKE_IMAGE'],'openlucky:old','openlucky:foreign']))
if args[:2]==['image','rm']: state['removed'].append(args[2]);save();sys.exit(0)
save();sys.exit(0)
'''


def main():
    for mode in ('success', 'start-failure', 'snapshot-failure', 'preserve-runtime'):
        with tempfile.TemporaryDirectory(prefix='openlucky-deploy-test-') as directory:
            root = Path(directory)
            for commit in (SHA, OLD_SHA):
                (root / 'releases' / commit / 'deploy').mkdir(parents=True)
                (root / 'releases' / commit / 'deploy/compose.yml').write_text('services: {}')
            if mode == 'preserve-runtime':
                (root / 'runtime.env').write_text('OPENLUCKY_ADMIN_ADDR=192.168.1.85:24150\n')
            (root / 'current').symlink_to(root / 'releases' / OLD_SHA)
            (root / 'data').mkdir()
            (root / 'data/marker').write_text('old-version')
            mock = root / 'docker'
            mock.write_text(MOCK); mock.chmod(0o700)
            (root / 'docker-state.json').write_text(json.dumps({'image': 'openlucky:old', 'running': True, 'calls': [], 'removed': [], 'admins': []}))
            env = dict(os.environ, FAKE_ROOT=directory, FAKE_MODE=mode, FAKE_SHA=SHA, FAKE_OLD_SHA=OLD_SHA, FAKE_IMAGE='openlucky:new')
            result = subprocess.run(['sh', str(ROOT / '.myself/activate-qnap.sh'), directory, SHA, 'openlucky:new', str(mock)], env=env, text=True, capture_output=True, timeout=20)
            state = json.loads((root / 'docker-state.json').read_text())
            assert state['running'], (mode, result.stderr)
            assert not (root / '.deployment-lock').exists(), mode
            assert not any('prune' in call for call in state['calls'])
            assert all(addr == ('192.168.1.85:24150' if mode == 'preserve-runtime' else '192.168.1.85:24050') for addr in state['admins']), state['admins']
            health_calls = [call for call in state['calls'] if call[0] == 'exec']
            assert all((':24150/' if mode == 'preserve-runtime' else ':24050/') in call[-1] for call in health_calls), health_calls
            if mode in ('success', 'preserve-runtime'):
                assert result.returncode == 0, result.stderr
                assert state['image'] == 'openlucky:new'
                assert 'openlucky:old' in state['removed'] and 'openlucky:foreign' not in state['removed']
                assert (root / 'current').resolve() == (root / 'releases' / SHA).resolve()
            else:
                assert result.returncode != 0
                assert state['image'] == 'openlucky:old'
                assert (root / 'data/marker').read_text() == 'old-version'
                assert state['removed'] == []
            print('PASS deployment', mode)


if __name__ == '__main__':
    main()
