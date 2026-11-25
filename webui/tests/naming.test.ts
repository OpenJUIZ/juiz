// この機械の名前の一覧から管理画面の住所を作る(#618)
import { test } from "node:test";
import assert from "node:assert/strict";
import type { NameRegistration } from "../src/api/types.ts";
import { registeredName, sortRegistrations, webUiUrl } from "../src/naming.ts";

function reg(name: string, namespace: string, brokers: NameRegistration["brokers"]): NameRegistration {
  return {
    format: 1,
    name,
    namespace,
    uuid: `${namespace}/${name}`,
    pid: 1,
    host: "pc",
    juiz_version: "0.3.6",
    started_at: "2026-11-25T06:00:00Z",
    brokers,
    machine: "this",
  };
}

test("registered name and order", () => {
  const a = reg("sim", "robot1", []);
  const b = reg("base", "", []);
  assert.equal(registeredName(a), "robot1/sim");
  assert.equal(registeredName(b), "base");
  assert.deepEqual(sortRegistrations([a, b]).map(registeredName), ["base", "robot1/sim"]);
});

test("web ui url uses the host of the juiz the list came from", () => {
  const r = reg("sim", "", [
    { type_name: "ipc", id: "ipc://sim" },
    { type_name: "http", id: "http://0.0.0.0:8001", port: 8001 },
  ]);
  assert.equal(webUiUrl(r, "", "http://robot1-pc:8000"), "http://robot1-pc:8001/ui/");
  assert.equal(webUiUrl(r, "http://192.168.0.5:8000", "http://localhost:5173"), "http://192.168.0.5:8001/ui/");
  const loop = reg("sim", "", [{ type_name: "http", id: "http://127.0.0.1:8003" }]);
  assert.equal(webUiUrl(loop, "", "http://robot1-pc:8000"), "http://robot1-pc:8003/ui/");
  const fixed = reg("sim", "", [{ type_name: "http", id: "http://10.0.0.2:8002" }]);
  assert.equal(webUiUrl(fixed, "", "http://localhost:8000"), "http://10.0.0.2:8002/ui/");
  assert.equal(webUiUrl(reg("sim", "", [{ type_name: "ipc", id: "ipc://sim" }]), "", "http://localhost:8000"), null);
});
