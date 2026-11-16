"""SystemProxy.container(id)・process(id) を名前や ID で探すテスト (issue #502)。juiz を起動せずに
    python test/unit/system_lookup_test.py
で実行する。list API の応答は偽の CRUD で返す。
"""
import sys
sys.path.append('.')
sys.path.append('..')

from juiz.proxy import (SystemProxy, HttpProxy, IpcProxy, ContainerProxy, ProcessProxy,
                        IdentifierStruct, AmbiguousNameError)


def _item(broker_type, broker_name, class_name, name, type_name, namespace=''):
    return {'broker_type_name': broker_type, 'broker_name': broker_name, 'class_name': class_name,
            'namespace': namespace, 'name': name, 'type_name': type_name}


class _FakeHttp(HttpProxy):
    """list だけ答える。broker_name は接続に使った Host (juiz の HTTP ブローカーと同じ)"""

    def __init__(self, host='localhost', port=8000):
        super().__init__(host, port)
        b = f'{host}:{port}'
        self._lists = {
            'container': [_item('http', b, 'container', 'camera0', 'static_anchor'),
                          _item('http', b, 'container', 'c0', 'counter'),
                          _item('http', b, 'container', 'c0', 'counter', 'robot1')],
            'process': [_item('http', b, 'process', 'print0', 'print'),
                        _item('http', b, 'process', 'inc0', 'increment', 'robot1'),
                        _item('http', b, 'process', 'inc0', 'increment', 'robot2')],
            'container_process': [dict(_item('http', b, 'container_process', 'c0get', 'get:c0'), container_name='c0'),
                                  dict(_item('http', b, 'container_process', 'c0inc', 'inc:c0'), container_name='c0')],
        }

    def read(self, class_name, function_name, param_dict={}):
        assert function_name == 'list', function_name
        return self._lists[class_name]


class _FakeIpc(IpcProxy):
    def __init__(self, name='juiz'):
        super().__init__(name)
        self._lists = {'container': [_item('ipc', name, 'container', 'camera0', 'static_anchor')],
                       'process': []}

    def read(self, class_name, function_name, param_dict={}):
        return self._lists[class_name]


def _cid(c):
    return None if c is None else c.identifier()


def check_container_by_name_and_id():
    s = SystemProxy(_FakeHttp('localhost', 8000))
    want = 'http://localhost:8000/container/camera0::static_anchor'
    for key in ['camera0',
                want,
                'http://127.0.0.1:8000/container/camera0::static_anchor',  # juiz container list が出す ID
                'http://[::1]:8000/container/camera0::static_anchor',
                'core://core/container/camera0::static_anchor',
                IdentifierStruct.fromIdentifier('core://core/container/camera0::static_anchor'),
                ContainerProxy.new(s, want)]:
        c = s.container(key)
        assert isinstance(c, ContainerProxy), key
        assert c.identifier() == want, (key, _cid(c))

    # 名前が同じでもネームスペースで分ける (ルートを優先)
    assert _cid(s.container('c0')) == 'http://localhost:8000/container/c0::counter'
    assert _cid(s.container('robot1/c0')) == 'http://localhost:8000/container/robot1/c0::counter'
    assert _cid(s.container('c0', namespace='robot1')) == 'http://localhost:8000/container/robot1/c0::counter'
    assert _cid(s.container('core://core/container/robot1/c0::counter')) == \
        'http://localhost:8000/container/robot1/c0::counter'

    # 見つからないもの
    for key in ['nothing0',
                'robot2/c0',
                'core://core/container/camera0::other_type',    # 型名が違う
                'core://core/process/camera0::static_anchor',   # class が違う
                'http://localhost:8001/container/camera0::static_anchor',  # 別の System
                'http://otherhost:8000/container/camera0::static_anchor',
                'ipc://juiz/container/camera0::static_anchor']:
        assert s.container(key) is None, key


def check_process_by_name_and_id():
    s = SystemProxy(_FakeHttp('127.0.0.1', 8000))
    want = 'http://127.0.0.1:8000/process/print0::print'
    for key in ['print0', want, 'http://localhost:8000/process/print0::print', 'core://core/process/print0::print']:
        p = s.process(key)
        assert isinstance(p, ProcessProxy), key
        assert p.identifier() == want, (key, _cid(p))

    assert _cid(s.process('robot2/inc0')) == 'http://127.0.0.1:8000/process/robot2/inc0::increment'
    assert _cid(s.process('inc0', namespace='robot1')) == 'http://127.0.0.1:8000/process/robot1/inc0::increment'
    try:
        s.process('inc0')  # robot1 と robot2 にあり、ルートには無い
        raise AssertionError('ambiguous name must fail')
    except AmbiguousNameError:
        pass
    assert s.process('core://core/process/inc0::increment') is None  # ルートには無い
    assert s.process('nothing0') is None


def check_ipc():
    s = SystemProxy(_FakeIpc('juiz'))
    want = 'ipc://juiz/container/camera0::static_anchor'
    for key in ['camera0', want, 'core://core/container/camera0::static_anchor']:
        assert _cid(s.container(key)) == want, key
    assert s.container('ipc://other/container/camera0::static_anchor') is None
    assert s.container('http://localhost:8000/container/camera0::static_anchor') is None


def check_container_process():
    s = SystemProxy(_FakeHttp())
    c0 = s.container('c0')
    want = 'http://localhost:8000/container_process/c0inc::inc:c0'
    for key in ['c0inc', want, 'core://core/container_process/c0inc::inc:c0',
                'http://127.0.0.1:8000/container_process/c0inc::inc:c0']:
        assert _cid(c0.process(key)) == want, key
    assert c0.process('nothing') is None


def check_self_broker():
    h = HttpProxy('localhost', 8000)
    assert h.is_self_broker('http', 'localhost:8000')
    assert h.is_self_broker('http', '127.0.0.1:8000')
    assert h.is_self_broker('http', '[::1]:8000')
    assert not h.is_self_broker('http', 'localhost:8001')
    assert not h.is_self_broker('http', 'otherhost:8000')
    assert not h.is_self_broker('http', 'localhost')
    assert not h.is_self_broker('qmp', 'localhost:8000')
    r = HttpProxy('robot.local', 8000)
    assert r.is_self_broker('http', 'robot.local:8000')
    assert r.is_self_broker('http', '127.0.0.1:8000')  # その System 自身が出した ID
    i = IpcProxy('juiz')
    assert i.is_self_broker('ipc', 'juiz')
    assert not i.is_self_broker('ipc', 'other')


def main():
    check_container_by_name_and_id()
    check_process_by_name_and_id()
    check_ipc()
    check_container_process()
    check_self_broker()
    print('ok')


if __name__ == '__main__':
    main()
