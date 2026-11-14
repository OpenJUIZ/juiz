// SDF の部分集合(container-model.md §3.2)を XML から読んで three.js の基本形にする。
// 読むのは <model>・<link>・<visual> の <pose> と、<visual> の <geometry>・<material> だけ。
import {
  BoxGeometry,
  CapsuleGeometry,
  CylinderGeometry,
  DoubleSide,
  type BufferGeometry,
  Group,
  Matrix4,
  Mesh,
  MeshStandardMaterial,
  PlaneGeometry,
  Quaternion,
  SphereGeometry,
  Vector3,
} from "three";
import { parseColor, sdfPoseMatrix } from "./placement.ts";

export interface SdfResult {
  /** 形を 1 つも作れなかったとき(<mesh> だけなど)は null */
  object: Group | null;
  /** 表示に関わる注意(<mesh> を表示しない、読めない要素など) */
  warnings: string[];
}

/** 直下の子要素(名前で絞る) */
function children(e: Element, name: string): Element[] {
  return Array.from(e.children).filter((c) => c.localName === name);
}

function child(e: Element, name: string): Element | undefined {
  return children(e, name)[0];
}

function numbers(e: Element | undefined): number[] | null {
  if (!e) return null;
  const v = (e.textContent ?? "").trim().split(/\s+/).map(Number);
  return v.every((x) => Number.isFinite(x)) ? v : null;
}

function number(e: Element | undefined): number | null {
  const v = numbers(e);
  return v && v.length === 1 ? v[0]! : null;
}

/** 要素の <pose>。無ければ単位。読めなければ単位にして注意を足す */
function poseOf(e: Element, where: string, warnings: string[]): Matrix4 {
  const p = child(e, "pose");
  if (!p) return new Matrix4();
  const relativeTo = p.getAttribute("relative_to");
  const parentName = e.parentElement?.getAttribute("name");
  if (relativeTo && relativeTo !== parentName) {
    warnings.push(`${where} の <pose relative_to="${relativeTo}"> は読まず、すぐ上の要素から見た位置にしました`);
  }
  const m = sdfPoseMatrix(p.textContent, p.getAttribute("degrees") === "true");
  if (!m) {
    warnings.push(`${where} の <pose> が「x y z roll pitch yaw」の 6 つの数ではありません`);
    return new Matrix4();
  }
  return m;
}

/** 3 つの数(<size> など)。読めなければ null */
function vec3(e: Element | undefined): [number, number, number] | null {
  const v = numbers(e);
  return v && v.length === 3 ? (v as [number, number, number]) : null;
}

/** three.js の円柱・カプセルは Y 軸向きなので、SDF の Z 軸向きに回す */
const Y_TO_Z = new Matrix4().makeRotationX(Math.PI / 2);

/** <geometry> を three.js の形にする。表示しない(<mesh>)・読めないものは null と理由 */
function geometryOf(g: Element): { geometry: BufferGeometry; matrix?: Matrix4; doubleSide?: boolean } | string {
  const shape = g.children[0];
  if (!shape) return "<geometry> が空です";
  switch (shape.localName) {
    case "box": {
      const s = vec3(child(shape, "size"));
      return s ? { geometry: new BoxGeometry(...s) } : "<box><size> が読めません";
    }
    case "cylinder": {
      const r = number(child(shape, "radius"));
      const l = number(child(shape, "length"));
      return r !== null && l !== null
        ? { geometry: new CylinderGeometry(r, r, l, 32), matrix: Y_TO_Z }
        : "<cylinder> の <radius>・<length> が読めません";
    }
    case "sphere": {
      const r = number(child(shape, "radius"));
      return r !== null ? { geometry: new SphereGeometry(r, 32, 16) } : "<sphere><radius> が読めません";
    }
    case "capsule": {
      const r = number(child(shape, "radius"));
      const l = number(child(shape, "length"));
      return r !== null && l !== null
        ? { geometry: new CapsuleGeometry(r, l, 8, 32), matrix: Y_TO_Z }
        : "<capsule> の <radius>・<length> が読めません";
    }
    case "ellipsoid": {
      const r = vec3(child(shape, "radii"));
      return r
        ? { geometry: new SphereGeometry(1, 32, 16), matrix: new Matrix4().makeScale(...r) }
        : "<ellipsoid><radii> が読めません";
    }
    case "plane": {
      const size = numbers(child(shape, "size"));
      const n = child(shape, "normal") ? vec3(child(shape, "normal")) : [0, 0, 1];
      if (!size || size.length !== 2 || !n) return "<plane> の <size>・<normal> が読めません";
      const normal = new Vector3(...n);
      if (normal.lengthSq() === 0) return "<plane><normal> の長さが 0 です";
      const q = new Quaternion().setFromUnitVectors(new Vector3(0, 0, 1), normal.normalize());
      return {
        geometry: new PlaneGeometry(size[0], size[1]),
        matrix: new Matrix4().makeRotationFromQuaternion(q),
        doubleSide: true,
      };
    }
    case "mesh":
      // 1 Container につき 1 ファイルしか配らないので、指す先を取り出せない(§3.2.1)
      return "<mesh> は表示しません(container-model.md §3.2.1)";
    default:
      return `<${shape.localName}> は対応していない形です`;
  }
}

/** <material> の <diffuse>(無ければ <ambient>)の RGBA */
function materialColor(visual: Element): { rgb: [number, number, number]; alpha: number } | null {
  const m = child(visual, "material");
  if (!m) return null;
  for (const name of ["diffuse", "ambient"]) {
    const v = numbers(child(m, name));
    if (v && (v.length === 3 || v.length === 4)) {
      return { rgb: [v[0]!, v[1]!, v[2]!], alpha: v[3] ?? 1 };
    }
  }
  return null;
}

/**
 * SDF の文を three.js の Group にする。座標は Container の座標(origin・U・S は呼ぶ側で掛ける)。
 * fallbackColor は conf の color("#RRGGBB(AA)")、defaultColor は表示側の既定の色
 */
export function buildSdf(text: string, fallbackColor: string, defaultColor: number): SdfResult {
  const warnings: string[] = [];
  const doc = new DOMParser().parseFromString(text, "application/xml");
  if (doc.getElementsByTagName("parsererror").length > 0) {
    return { object: null, warnings: ["SDF を XML として読めません"] };
  }
  const sdf = doc.documentElement;
  const models = sdf.localName === "sdf" ? children(sdf, "model") : [];
  if (models.length !== 1) {
    return { object: null, warnings: ["<sdf> の直下に <model> が 1 つではありません"] };
  }
  const model = models[0]!;
  if (children(model, "model").length > 0) warnings.push("入れ子の <model> は表示しません");
  if (children(model, "joint").length > 0) {
    warnings.push("<joint> は読まず、link を固定で置きました(動く部分は Container を分けます)");
  }
  const conf = parseColor(fallbackColor);
  const root = new Group();
  root.name = model.getAttribute("name") ?? "model";
  root.matrixAutoUpdate = false;
  root.matrix.copy(poseOf(model, "<model>", warnings));
  for (const link of children(model, "link")) {
    const linkName = link.getAttribute("name") ?? "";
    const linkGroup = new Group();
    linkGroup.name = linkName;
    linkGroup.matrixAutoUpdate = false;
    linkGroup.matrix.copy(poseOf(link, `<link name="${linkName}">`, warnings));
    for (const visual of children(link, "visual")) {
      const where = `<visual name="${visual.getAttribute("name") ?? ""}">`;
      const geometry = child(visual, "geometry");
      if (!geometry) continue;
      const g = geometryOf(geometry);
      if (typeof g === "string") {
        warnings.push(`${where}: ${g}`);
        continue;
      }
      const c = materialColor(visual);
      const material = new MeshStandardMaterial({ color: c ? 0 : (conf?.rgb ?? defaultColor) });
      if (g.doubleSide) material.side = DoubleSide;
      if (c) material.color.setRGB(...c.rgb);
      const alpha = c ? c.alpha : (conf?.alpha ?? 1);
      if (alpha < 1) {
        material.transparent = true;
        material.opacity = alpha;
      }
      const mesh = new Mesh(g.geometry, material);
      mesh.name = visual.getAttribute("name") ?? "";
      mesh.matrixAutoUpdate = false;
      mesh.matrix.copy(poseOf(visual, where, warnings));
      if (g.matrix) mesh.matrix.multiply(g.matrix);
      linkGroup.add(mesh);
    }
    if (linkGroup.children.length > 0) root.add(linkGroup);
  }
  return { object: root.children.length > 0 ? root : null, warnings };
}
