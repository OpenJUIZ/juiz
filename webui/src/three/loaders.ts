// Container の 3D モデルを取り出して three.js の形にする(container-model.md §12)。
// 同じファイル(SHA-256)は 1 回だけ取って読み、Container ごとには写し(clone)を使う。
import { Box3, BufferGeometry, Mesh, MeshStandardMaterial, type Object3D } from "three";
import { GLTFLoader } from "three/examples/jsm/loaders/GLTFLoader.js";
import { STLLoader } from "three/examples/jsm/loaders/STLLoader.js";
import type { JuizClient } from "../api/client.ts";
import type { ContainerModelInfo } from "../api/types.ts";
import { parseColor } from "./placement.ts";
import { buildSdf } from "./sdf.ts";

/** 色を持たない形式・要素の既定の色 */
export const DEFAULT_MODEL_COLOR = 0x9fb4bc;

/** 表示できる形式か(other は名前と座標軸だけ、§3.1) */
export function isDisplayable(format: string): boolean {
  return ["glb", "gltf", "stl", "sdf"].includes(format);
}

export interface LoadedModel {
  /** 表示する形。作れなければ null(座標軸と名前だけ出す) */
  object: Object3D | null;
  /** 表示に関わる注意 */
  warnings: string[];
}

function stlObject(data: ArrayBuffer, color: string): Object3D {
  const geometry: BufferGeometry = new STLLoader().parse(data);
  // 面の法線が 0 のまま書かれた STL は少なくないので、長さの無い法線があれば形から求め直す
  const normal = geometry.getAttribute("normal");
  let broken = !normal;
  for (let i = 0; normal && !broken && i < normal.count; i++) {
    broken = normal.getX(i) ** 2 + normal.getY(i) ** 2 + normal.getZ(i) ** 2 < 0.25;
  }
  if (broken) geometry.computeVertexNormals();
  const c = parseColor(color);
  const material = new MeshStandardMaterial({ color: c?.rgb ?? DEFAULT_MODEL_COLOR });
  // バイナリ STL の面の色(VisCAM/SolidView の形)があればそれを使う
  if (geometry.getAttribute("color")) material.vertexColors = true;
  if (c && c.alpha < 1) {
    material.transparent = true;
    material.opacity = c.alpha;
  }
  return new Mesh(geometry, material);
}

async function parse(info: ContainerModelInfo, data: ArrayBuffer): Promise<LoadedModel> {
  switch (info.format) {
    case "glb":
    case "gltf": {
      // 外部のファイルを参照する .gltf は配れない(§13)ので、中に埋め込んだものだけ読める
      const gltf = await new GLTFLoader().parseAsync(data, "");
      return { object: gltf.scene, warnings: [] };
    }
    case "stl":
      return { object: stlObject(data, info.color), warnings: [] };
    case "sdf": {
      const r = buildSdf(new TextDecoder().decode(data), info.color, DEFAULT_MODEL_COLOR);
      return { object: r.object, warnings: r.warnings };
    }
    default:
      return { object: null, warnings: [`形式 ${info.format} は表示しません`] };
  }
}

/** 形の外接箱の一辺の最大 [ファイルの単位] */
export function maxExtent(object: Object3D): number {
  object.updateMatrixWorld(true);
  const box = new Box3().setFromObject(object);
  if (box.isEmpty()) return 0;
  const size = box.getSize(box.min.clone());
  return Math.max(size.x, size.y, size.z);
}

/**
 * モデルの読み込みの使い回し。鍵は SHA-256 と、形の作り方を変える値(形式・色)。
 * 同じ形の Container が 10 台あっても、取り出しと読み込みは 1 回
 */
export class ModelCache {
  private readonly client: JuizClient;
  private readonly entries = new Map<string, Promise<LoadedModel>>();

  constructor(client: JuizClient) {
    this.client = client;
  }

  static keyOf(info: ContainerModelInfo): string {
    return `${info.sha256}|${info.format}|${info.format === "glb" || info.format === "gltf" ? "" : info.color}`;
  }

  /** identifier の Container のモデルを読む。返す形は写し(呼んだ側が自由に置いてよい) */
  async load(identifier: string, info: ContainerModelInfo): Promise<LoadedModel> {
    if (!isDisplayable(info.format)) {
      return { object: null, warnings: [`形式 ${info.format} は表示しません`] };
    }
    const key = ModelCache.keyOf(info);
    let entry = this.entries.get(key);
    if (!entry) {
      entry = this.client.containerModel(identifier).then(({ data }) => parse(info, data));
      this.entries.set(key, entry);
      // 失敗したら次に呼ばれたときに取り直す
      entry.catch(() => {
        if (this.entries.get(key) === entry) this.entries.delete(key);
      });
    }
    const loaded = await entry;
    return { object: loaded.object ? loaded.object.clone() : null, warnings: loaded.warnings };
  }
}
