"""ID の分解のテスト (issue #152)。juiz を起動せずに
    python test/identifier_test.py
で実行する。
"""
import sys
sys.path.append('.')
sys.path.append('..')

from juiz.proxy import (IdentifierStruct, to_identifier, id_to_name, id_to_namespace, id_to_full_name,
                        id_to_type_name, split_namespace, join_namespace, is_in_namespace)


def check_root():
    i = 'core://core/process/inc0::increment'
    assert id_to_name(i) == 'inc0'
    assert id_to_namespace(i) == ''
    assert id_to_full_name(i) == 'inc0'
    assert id_to_type_name(i) == 'increment'
    assert IdentifierStruct.fromIdentifier(i).to_id() == i


def check_namespaced():
    i = 'core://core/process/robot1/arm/inc0::increment'
    s = IdentifierStruct.fromIdentifier(i)
    assert (s.broker_type, s.broker_name, s.class_name) == ('core', 'core', 'process')
    assert (s.namespace, s.name, s.type_name) == ('robot1/arm', 'inc0', 'increment')
    assert s.full_name() == 'robot1/arm/inc0'
    assert s.to_id() == i
    assert id_to_name(i) == 'inc0'
    assert id_to_namespace(i) == 'robot1/arm'


def check_broker_and_container_process():
    # ブローカー名に : が入り、ContainerProcess の型名は "型:所属 Container 名"
    i = 'http://127.0.0.1:8000/container_process/robot1/c0inc::example_container_increment:c0'
    s = IdentifierStruct.fromIdentifier(i)
    assert s.broker_name == '127.0.0.1:8000'
    assert (s.namespace, s.name) == ('robot1', 'c0inc')
    assert s.type_name == 'example_container_increment:c0'
    assert s.to_id() == i


def check_list_element():
    # list API の各要素 (ProcessIdentifier の JSON)
    v = {'broker_type_name': 'core', 'broker_name': 'core', 'class_name': 'process',
         'namespace': 'robot1', 'name': 'inc0', 'type_name': 'increment', 'container_name': None}
    assert to_identifier(v) == 'core://core/process/robot1/inc0::increment'
    v['namespace'] = ''
    assert to_identifier(v) == 'core://core/process/inc0::increment'
    # namespace の無い古い juiz の応答
    del v['namespace']
    assert to_identifier(v) == 'core://core/process/inc0::increment'
    assert id_to_namespace(v) == ''


def check_helpers():
    assert split_namespace('robot1/arm/inc0') == ('robot1/arm', 'inc0')
    assert split_namespace('inc0') == ('', 'inc0')
    assert join_namespace('', 'inc0') == 'inc0'
    assert join_namespace('robot1', 'inc0') == 'robot1/inc0'
    assert is_in_namespace('robot1/arm', 'robot1')
    assert is_in_namespace('robot1', '/robot1')
    assert is_in_namespace('robot1', '')
    assert not is_in_namespace('robot10', 'robot1')
    assert not is_in_namespace('', 'robot1')


def check_invalid():
    try:
        IdentifierStruct.fromIdentifier('inc0')
        raise AssertionError('an identifier without :// must fail')
    except ValueError:
        pass


def main():
    check_root()
    check_namespaced()
    check_broker_and_container_process()
    check_list_element()
    check_helpers()
    check_invalid()
    print('ok')


if __name__ == '__main__':
    main()
