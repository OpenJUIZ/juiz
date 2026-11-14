// Container の 3D モデルを置く位置の計算(container-model.md §4)。
// three.js の数学の型だけを使う(描画には触れない)ので、node のテスト(tests/placement.test.ts)でも動く。
import { Matrix4, Quaternion, Vector3 } from "three";
import type { ContainerModelInfo, Pose, TransformValue } from "../api/types.ts";

/** x 軸まわり +90°(glTF などの Y 上を juiz の Z 上にそろえる、§4.2) */
const Y_UP_TO_Z_UP = new Quaternion(Math.SQRT1_2, 0, 0, Math.SQRT1_2);

/** 位置姿勢(Pose)を同次変換の行列にする。四元数は正規化して使う */
export function poseMatrix(pose: Pose): Matrix4 {
  const { position: p, orientation: q } = pose;
  return new Matrix4().compose(
    new Vector3(p.x, p.y, p.z),
    new Quaternion(q.x, q.y, q.z, q.w).normalize(),
    new Vector3(1, 1, 1),
  );
}

/** Transform(translation・rotation)を行列にする */
export function transformMatrix(t: TransformValue): Matrix4 {
  return poseMatrix({ position: t.translation, orientation: t.rotation });
}

/** 形式の変換 U(§4.2)。up_axis が y なら x 軸まわり +90°、それ以外は回さない */
export function upAxisQuaternion(upAxis: string): Quaternion {
  return upAxis === "y" ? Y_UP_TO_Z_UP.clone() : new Quaternion();
}

/**
 * Container の座標から見たモデルの置き方 origin ∘ U ∘ S(§4.4)。
 * scale が正の有限の数でなければ 1 として扱う(core が作成時に断るので、ふつうは来ない)
 */
export function modelLocalMatrix(info: Pick<ContainerModelInfo, "origin" | "up_axis" | "scale">): Matrix4 {
  const s = Number.isFinite(info.scale) && info.scale > 0 ? info.scale : 1;
  const us = new Matrix4().compose(new Vector3(), upAxisQuaternion(info.up_axis), new Vector3(s, s, s));
  return transformMatrix(info.origin).multiply(us);
}

/** Anchor から見たモデルの置き方 pose_A(C) ∘ origin ∘ U ∘ S(§4.4) */
export function modelMatrix(pose: Pose, info: Pick<ContainerModelInfo, "origin" | "up_axis" | "scale">): Matrix4 {
  return poseMatrix(pose).multiply(modelLocalMatrix(info));
}

/** モデルのファイルの中の点 p_m を、Anchor から見た点 p_A に写す(§4.4 の式) */
export function placePoint(
  pose: Pose,
  info: Pick<ContainerModelInfo, "origin" | "up_axis" | "scale">,
  p: { x: number; y: number; z: number },
): Vector3 {
  return new Vector3(p.x, p.y, p.z).applyMatrix4(modelMatrix(pose, info));
}

/**
 * SDF の <pose> の文(x y z roll pitch yaw)を行列にする(§3.2)。
 * 回転は R = Rz(yaw)·Ry(pitch)·Rx(roll)。degrees なら角度は度。
 * 空なら単位、数が 6 つでなければ null(書き間違い)
 */
export function sdfPoseMatrix(text: string | null | undefined, degrees = false): Matrix4 | null {
  const t = (text ?? "").trim();
  if (t === "") return new Matrix4();
  const v = t.split(/\s+/).map(Number);
  if (v.length !== 6 || v.some((x) => !Number.isFinite(x))) return null;
  const [x, y, z, roll, pitch, yaw] = v as [number, number, number, number, number, number];
  const k = degrees ? Math.PI / 180 : 1;
  const r = new Matrix4()
    .makeRotationZ(yaw * k)
    .multiply(new Matrix4().makeRotationY(pitch * k))
    .multiply(new Matrix4().makeRotationX(roll * k));
  return r.setPosition(x, y, z);
}

/** "#RRGGBB" / "#RRGGBBAA" を色(0xRRGGBB)と不透明度にする。読めなければ null */
export function parseColor(s: string | null | undefined): { rgb: number; alpha: number } | null {
  const m = /^#([0-9a-f]{6})([0-9a-f]{2})?$/i.exec((s ?? "").trim());
  if (!m) return null;
  return { rgb: parseInt(m[1]!, 16), alpha: m[2] ? parseInt(m[2], 16) / 255 : 1 };
}
