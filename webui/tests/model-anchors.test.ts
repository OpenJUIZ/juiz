// つながった系の Anchor(src/model.ts の connectedAnchorContainers、#567)のテスト。npm test
import assert from "node:assert/strict";
import { test } from "node:test";
import type { SystemProfile } from "../src/api/types.ts";
import { buildModel, connectedAnchorContainers, containersById } from "../src/model.ts";

const container = (name: string, type_name: string, anchor: boolean) => ({
  name,
  type_name,
  identifier: `core://core/container/${name}::${type_name}`,
  ...(anchor ? { anchor: { rate: 0 } } : {}),
});

const store = (containers: unknown[]) => ({ processes: [], containers, container_processes: [], ecs: [], brokers: [] });

test("副系・親系の Anchor は最上位の系から見た識別子で出る", () => {
  // teleop_twist(Anchor なし)の親系に mobile_sim(world・odom0)
  const profile = {
    name: "teleop",
    type_name: "System",
    system_store: { uuid: "u-teleop" },
    core_store: store([container("teleop0", "teleop", false)]),
    subsystems: [],
    mastersystem: [
      {
        name: "mobile_sim",
        type_name: "System",
        system_store: { uuid: "u-sim" },
        broker_proxy: { name: "mobile_sim", type_name: "ipc" },
        core_store: store([
          container("world", "static_anchor", true),
          container("odom0", "static_anchor", true),
          container("robot0", "frame", false),
        ]),
        subsystems: [{ visited: true, system_uuid: "u-teleop" }],
      },
    ],
    connections: [],
  } as unknown as SystemProfile;
  const model = buildModel(profile, "http://localhost:8002");
  const anchors = connectedAnchorContainers(model).map(({ system, container }) => [system.name, container.identifier]);
  assert.deepEqual(anchors, [
    ["mobile_sim", "ipc://mobile_sim/container/odom0::static_anchor"],
    ["mobile_sim", "ipc://mobile_sim/container/world::static_anchor"],
  ]);
  // 木のノード(副系から回ってきた識別子)から容器を引ける
  const byId = containersById(model);
  assert.equal(byId.get("ipc://mobile_sim/container/robot0::frame")?.fullName, "robot0");
  assert.equal(byId.get("core://core/container/teleop0::teleop")?.fullName, "teleop0");
});
