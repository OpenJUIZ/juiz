"""名前の一覧から juiz を引くテスト (docs/spec/naming.md §9.4、issue #618)。juiz を起動せずに
    python test/unit/naming_test.py
で実行する。登録ファイルは一時ディレクトリ (JUIZ_IPC_DIR) に書き、system/profile の問い合わせは差し替える。
"""
import json
import os
import shutil
import sys
import tempfile
sys.path.append('.')
sys.path.append('..')

from juiz import naming
from juiz.naming import NameQuery, NameNotRegisteredError, NameAmbiguousError
from juiz import proxy

DEAD_PID = 2_000_000_000


def _write(names, ns, name, uuid, pid, brokers):
    d = os.path.join(names, *[s for s in ns.split('/') if s])
    os.makedirs(d, exist_ok=True)
    reg = {'format': 1, 'name': name, 'namespace': ns, 'uuid': uuid, 'pid': pid, 'host': 'pc',
           'juiz_version': '0.3.6', 'started_at': '2026-11-25T06:00:00Z', 'brokers': brokers}
    with open(os.path.join(d, name + '.json'), 'w', encoding='utf-8') as f:
        json.dump(reg, f)


def test_query_parse():
    q = NameQuery.parse('mobile_sim')
    assert (q.name, q.namespace, q.host, q.kind) == ('mobile_sim', None, None, None)
    q = NameQuery.parse('robot1/mobile_sim')
    assert (q.name, q.namespace) == ('mobile_sim', 'robot1')
    assert NameQuery.parse('/mobile_sim').namespace == ''
    assert NameQuery.parse('mobile_sim@pc').host == 'pc'
    q = NameQuery.parse('name://mobile_sim?ns=robot1&type=http')
    assert (q.name, q.namespace, q.kind) == ('mobile_sim', 'robot1', 'http')
    assert NameQuery.parse('name://mobile_sim?ns=/').namespace == ''
    for bad in ['name://robot1/mobile_sim', 'name://a?port=1', 'my sys', 'http://x:1', 'a@']:
        try:
            NameQuery.parse(bad)
        except ValueError:
            continue
        raise AssertionError(bad)
    assert naming.is_name('mobile_sim') and naming.is_name('name://a')
    assert not naming.is_name('http://127.0.0.1:8000') and not naming.is_name('localhost:8000')


def test_local_url():
    assert naming.local_url({'id': 'http://0.0.0.0:8001'}) == 'http://127.0.0.1:8001'
    assert naming.local_url({'id': 'http://192.168.0.2:8001'}) == 'http://192.168.0.2:8001'
    assert naming.local_url({'id': 'ipc://sim'}) == 'ipc://sim'


def test_resolve_and_list():
    tmp = tempfile.mkdtemp(prefix='pyjuiz-naming-')
    old_env = os.environ.get('JUIZ_IPC_DIR')
    old_probe = naming._probe
    os.environ['JUIZ_IPC_DIR'] = tmp
    try:
        names = os.path.join(tmp, 'names')
        me = os.getpid()
        both = [{'type_name': 'ipc', 'id': 'ipc://sim1'},
                {'type_name': 'http', 'id': 'http://0.0.0.0:8001', 'port': 8001}]
        _write(names, 'robot1', 'sim', 'u1', me, both)
        _write(names, 'robot2', 'sim', 'u2', me, [{'type_name': 'http', 'id': 'http://0.0.0.0:8002', 'port': 8002}])
        _write(names, '', 'dead', 'u3', DEAD_PID, both)
        _write(names, '', 'silent', 'u4', me, [{'type_name': 'http', 'id': 'http://0.0.0.0:8004'}])
        _write(names, '', 'zonly', 'u5', me, [{'type_name': 'zenoh', 'id': 'zenoh://0.0.0.0:7448'}])
        answers = {'ipc://sim1': 'u1', 'http://127.0.0.1:8001': 'u1', 'http://127.0.0.1:8002': 'u2'}
        naming._probe = lambda b: answers.get(naming.local_url(b))

        assert naming.resolve_name('robot1/sim') == 'ipc://sim1'
        assert naming.resolve_name('name://sim?ns=robot1&type=http') == 'http://127.0.0.1:8001'
        assert naming.resolve_name('robot2/sim') == 'http://127.0.0.1:8002'
        try:
            naming.resolve_name('sim')
            raise AssertionError('ambiguous')
        except NameAmbiguousError as e:
            assert str(e).startswith('sim is ambiguous: robot1/sim (this machine), robot2/sim (this machine).'), e
        for n, msg in [('dead', 'dead is not registered on this machine.'),
                       ('silent', 'silent is not registered on this machine.'),
                       ('zonly', 'zonly is not registered on this machine.'),
                       ('name://robot2_sim', 'robot2_sim is not registered on this machine.')]:
            try:
                naming.resolve_name(n)
                raise AssertionError(n)
            except NameNotRegisteredError as e:
                assert str(e) == msg, e
        try:
            naming.resolve_name('name://sim?ns=robot2&type=ipc')
            raise AssertionError('no ipc')
        except NameNotRegisteredError as e:
            assert str(e) == 'robot2/sim has no ipc broker (it has: http)', e

        listed = naming.name_list()
        assert [naming.registered_name(r) for r in listed] == ['robot1/sim', 'robot2/sim', 'silent', 'zonly'], listed
        assert all(r['machine'] == 'this' for r in listed)
        checked = {naming.registered_name(r): r['alive'] for r in naming.name_list(check=True)}
        assert checked == {'robot1/sim': True, 'robot2/sim': True, 'silent': False, 'zonly': False}, checked

        # connect と ProcessProxy(protocol='name') は名前を引いた URL に繋ぐ
        s = proxy.connect('robot1/sim')
        assert repr(s.crud) == 'IpcProxy("ipc://sim1")'
        s = proxy.connect('name://sim?ns=robot2')
        assert s.crud.broker_id() == 'http://127.0.0.1:8002'
        system, address = proxy._connect_system('robot2/sim', None, 'name', None)
        assert address == 'http://127.0.0.1:8002'
    finally:
        naming._probe = old_probe
        if old_env is None:
            os.environ.pop('JUIZ_IPC_DIR', None)
        else:
            os.environ['JUIZ_IPC_DIR'] = old_env
        shutil.rmtree(tmp, ignore_errors=True)


if __name__ == '__main__':
    test_query_parse()
    test_local_url()
    test_resolve_and_list()
    print('naming_test: ok')
