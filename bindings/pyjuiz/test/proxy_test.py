"""HTTP Broker を立てた juiz に pyjuiz のプロキシで繋ぐテスト (issue #79)

juiz_core/tests/pyjuiz_proxy_test.rs が System を起動してから、このスクリプトを
    python test/proxy_test.py <port>
で実行する。プロセス inc0 (arg1 + 1 を返す) と、コンテナ c0 (c0inc で加算、c0get で取得) がある前提。
"""
import sys

from juiz.proxy import ProcessProxy, ContainerProxy, NotFoundError


def check_process(port):
    inc = ProcessProxy("inc0", "localhost", port)
    assert inc.name() == "inc0", inc.name()
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


def check_container(port):
    c0 = ContainerProxy("c0", "localhost", port)
    assert c0.name() == "c0", c0.name()
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
    print("ok")


if __name__ == "__main__":
    main()
