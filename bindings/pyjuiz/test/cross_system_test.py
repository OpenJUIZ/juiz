"""System をまたぐ Transform (issue #415、docs/spec/transformations.md §19) を pyjuiz で確かめる。

S1 (ポート argv[1]) に Anchor 'map'、S2 (ポート argv[2]) に 'odom'・'base0'・'c2' がある。S2 は S1 をマスターシステムにしている。
数値は §3.5 の例 (map → odom は (1, 0, 0) と z 軸まわり 90°、odom → base0 は (0.1, 0, 0.3))。
juiz_core/tests/integration/pyjuiz_cross_system_test.rs から実行される。
"""
import sys
import time

from juiz.proxy import SystemProxy, HttpProxy, ContainerProxy, InternalServerError

H = 0.7071067811865476


def _close(a, b, eps=1e-9):
    return all(abs(a[k] - b[k]) < eps for k in b)


def wait_pose(system, container, reference):
    """写しが届くまで (1 回目の取り込みまでは NoCommonAnchorError) 待って読む"""
    deadline = time.time() + 10.0
    while True:
        try:
            return system.pose(container, reference)
        except InternalServerError:
            if time.time() > deadline:
                raise
            time.sleep(0.1)


def main():
    p1, p2 = int(sys.argv[1]), int(sys.argv[2])
    s1 = SystemProxy(HttpProxy("localhost", p1))
    s2 = SystemProxy(HttpProxy("localhost", p2))
    s1map = ContainerProxy("map", "localhost", p1)
    base0 = ContainerProxy("base0", "localhost", p2)
    c2 = ContainerProxy("c2", "localhost", p2)

    # S2 に、始点を S1 の map (別 System の ContainerProxy) にした Transform を作る
    t = s2.create_transform(s1map, "odom", translation=(1, 0, 0), rotation=(0, 0, H, H), rate=20, timeout=1.0)
    assert t["source"] == f"http://localhost:{p1}/container/map::static_anchor", t
    assert t["remote"]["rate"] == 20.0 and t["remote"]["timeout"] == 1.0, t
    s2.create_transform("odom", "base0", translation=(0.1, 0, 0.3))

    # S2 で map (写し) から見た base0
    p = wait_pose(s2, "base0", "map")
    assert _close(p["pose"]["position"], {"x": 1.0, "y": 0.1, "z": 0.3}), p
    assert p["header"]["frame_id"] == t["source"], p
    assert s2.transform(t["identifier"])["remote"]["state"] == "ok"
    # 基準に別 System の ContainerProxy を渡す (S2 の写しの ID になる)
    assert _close(base0.pose(s1map)["pose"]["position"], {"x": 1.0, "y": 0.1, "z": 0.3})

    # S1 から: S2 の base0 を map から見る (S2 に回す) と、その逆 (S2 に頼んで逆にする)
    p = s1.pose(base0, "map")
    assert _close(p["pose"]["position"], {"x": 1.0, "y": 0.1, "z": 0.3}), p
    assert p["header"]["frame_id"] == "map", p
    p = s1map.pose(base0)
    assert _close(p["pose"]["position"], {"x": -0.1, "y": 1.0, "z": -0.3}), p

    # PoseState と、取りに来ている System (remote_children)
    st = s1map.pose_state()
    assert st["identifier"] == "core://core/container/map::static_anchor", st
    assert len(st["entries"]) == 1 and st["changed"], st
    same = s1.pose_state("map")
    assert same["version"] == st["version"], same
    children = s1map.profile_full(update=True).get("remote_children")
    assert children and children[0]["destinations"] == ["core://core/container/odom::example_container"], children

    # S1 から作る: 終点が S2 の Container なので S2 にできる。S1 には何も残らない
    t2 = s1.create_transform("map", c2, translation=(0, 0, 1))
    assert t2["destination"] == "core://core/container/c2::example_container", t2
    assert t2["identifier"] in s2.transforms() and s1.transforms() == []
    assert _close(wait_pose(s2, "c2", "map")["pose"]["position"], {"x": 0.0, "y": 0.0, "z": 1.0})
    s2.destroy_transform(t2["identifier"])
    print("ok")


if __name__ == "__main__":
    main()
