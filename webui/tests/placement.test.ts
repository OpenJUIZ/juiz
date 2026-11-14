// 3D 表示の位置の計算(src/three/placement.ts)のテスト。container-model.md §4.4 の数値の例を確かめる。
//   npm test(node --test、型は node が読み飛ばす)
import assert from "node:assert/strict";
import { test } from "node:test";
import { Vector3 } from "three";
import { parseColor, placePoint, sdfPoseMatrix } from "../src/three/placement.ts";

const EPS = 1e-9;

function near(actual: Vector3, expected: [number, number, number]) {
  const [x, y, z] = expected;
  assert.ok(
    Math.abs(actual.x - x) < EPS && Math.abs(actual.y - y) < EPS && Math.abs(actual.z - z) < EPS,
    `(${actual.x}, ${actual.y}, ${actual.z}) は (${x}, ${y}, ${z}) ではない`,
  );
}

const identity = { translation: { x: 0, y: 0, z: 0 }, rotation: { x: 0, y: 0, z: 0, w: 1 } };
// pose_A(C): position (1, 0, 0)、回転なし
const pose = { position: { x: 1, y: 0, z: 0 }, orientation: { x: 0, y: 0, z: 0, w: 1 } };

test("glTF(up_axis y)は x 軸まわり +90° で Y 上を Z 上にする(§4.4 の例 1)", () => {
  const info = { origin: { ...identity, translation: { x: 0, y: 0, z: 0.5 } }, up_axis: "y", scale: 1 };
  near(placePoint(pose, info, { x: 0, y: 1, z: 0 }), [1, 0, 1.5]);
  near(placePoint(pose, info, { x: 0, y: 0, z: 1 }), [1, -1, 0.5]);
});

test("STL の scale 0.001 は mm を m にする(§4.4 の例 2)", () => {
  const info = { origin: identity, up_axis: "z", scale: 0.001 };
  near(placePoint(pose, info, { x: 100, y: 0, z: 0 }), [1.1, 0, 0]);
});

test("pose の回転は origin・U・S の後に掛かる", () => {
  // z 軸まわり +90°
  const rotated = { position: { x: 0, y: 0, z: 0 }, orientation: { x: 0, y: 0, z: Math.SQRT1_2, w: Math.SQRT1_2 } };
  const info = { origin: { ...identity, translation: { x: 1, y: 0, z: 0 } }, up_axis: "z", scale: 2 };
  // p_m (1, 0, 0) → S (2, 0, 0) → origin (3, 0, 0) → 回転 (0, 3, 0)
  near(placePoint(rotated, info, { x: 1, y: 0, z: 0 }), [0, 3, 0]);
});

test("SDF の <pose> は R = Rz(yaw)·Ry(pitch)·Rx(roll)", () => {
  const m = sdfPoseMatrix("1 2 3 0 0 1.5707963267948966")!;
  near(new Vector3(1, 0, 0).applyMatrix4(m), [1, 3, 3]);
  // roll 90° の後に yaw 90°: +Y は roll で +Z へ、yaw では動かない
  const m2 = sdfPoseMatrix("0 0 0 90 0 90", true)!;
  near(new Vector3(0, 1, 0).applyMatrix4(m2), [0, 0, 1]);
  near(new Vector3(1, 0, 0).applyMatrix4(m2), [0, 1, 0]);
  assert.ok(sdfPoseMatrix("")!.equals(sdfPoseMatrix("0 0 0 0 0 0")!));
  assert.equal(sdfPoseMatrix("1 2 3"), null);
});

test("色は #RRGGBB と #RRGGBBAA", () => {
  assert.deepEqual(parseColor("#404040"), { rgb: 0x404040, alpha: 1 });
  assert.deepEqual(parseColor("#ff000080"), { rgb: 0xff0000, alpha: 128 / 255 });
  assert.equal(parseColor(""), null);
  assert.equal(parseColor("red"), null);
});
