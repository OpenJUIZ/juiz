"""HTTP Broker を立てた juiz に pyjuiz のプロキシで繋ぐテスト (issue #79)

juiz_core/tests/pyjuiz_proxy_test.rs が System を起動してから、このスクリプトを
    python test/proxy_test.py <port>
で実行する。プロセス inc0 (arg1 + 1 を返す) と、コンテナ c0 (c0inc で加算、c0get で取得) がある前提。
ネームスペース robot1 にも同じ名前の inc0 と c0 がある (issue #152)。
"""
import sys

from juiz.proxy import ProcessProxy, ContainerProxy, SystemProxy, HttpProxy, NotFoundError


def check_process(port):
    # 同じ名前がルートと robot1 にあるときは、ルートを選ぶ
    inc = ProcessProxy("inc0", "localhost", port)
    assert inc.name() == "inc0", inc.name()
    assert inc.namespace() == "", inc.namespace()
    assert inc.type_name() == "increment_process", inc.type_name()
    assert inc.identifier().endswith("/process/inc0::increment_process"), inc.identifier()
    assert inc.call(10) == 11
    assert inc(20) == 21
    inc.p_apply("arg1", 1)
    assert inc.execute() == 2

    try:
        ProcessProxy("nothing0", "localhost", port)
        raise AssertionError("ProcessProxy for a missing process must fail")
    except NotFoundError:
        pass


def check_namespaced_process(port):
    for inc in [ProcessProxy("inc0", "localhost", port, namespace="robot1"),
                ProcessProxy("robot1/inc0", "localhost", port)]:
        assert inc.name() == "inc0", inc.name()
        assert inc.namespace() == "robot1", inc.namespace()
        assert inc.full_name() == "robot1/inc0", inc.full_name()
        assert inc.identifier().endswith("/process/robot1/inc0::increment_process"), inc.identifier()
        assert inc.call(10) == 11

    try:
        ProcessProxy("inc0", "localhost", port, namespace="robot2")
        raise AssertionError("ProcessProxy in a missing namespace must fail")
    except NotFoundError:
        pass

    system = SystemProxy(HttpProxy("localhost", port))
    names = sorted(p.full_name() for p in system.processes(namespace="robot1"))
    assert names == ["robot1/inc0"], names
    assert "robot1/inc0" in [p.full_name() for p in system.processes()]


def check_namespaced_container(port):
    c0 = ContainerProxy("c0", "localhost", port, namespace="robot1")
    assert c0.full_name() == "robot1/c0", c0.full_name()
    ps = c0.processes()
    names = sorted(p.full_name() for p in ps)
    assert names == ["robot1/c0get", "robot1/c0inc"], names
    before = c0.c0get.call()
    assert c0.c0inc.call(3) == before + 3
    assert c0.c0get() == before + 3


def check_container(port):
    c0 = ContainerProxy("c0", "localhost", port)
    assert c0.name() == "c0", c0.name()
    assert c0.namespace() == "", c0.namespace()
    names = sorted(p.name() for p in c0.processes())
    assert names == ["c0get", "c0inc"], names
    before = c0.c0get.call()
    assert c0.c0inc.call(4) == before + 4
    assert c0.c0get() == before + 4
    c0.c0inc.p_apply("arg1", 1)
    assert c0.c0inc.execute() == before + 5


def main():
    port = int(sys.argv[1])
    check_process(port)
    check_container(port)
    check_namespaced_process(port)
    check_namespaced_container(port)
    print("ok")


if __name__ == "__main__":
    main()
