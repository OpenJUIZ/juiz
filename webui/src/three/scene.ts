// 3D 表示(container-model.md §12)の three.js の場面。React の外で three.js の物を持ち、
// ModelView.tsx が Anchor の木を取るたびに update を呼ぶ。
import {
  AmbientLight,
  AxesHelper,
  Box3,
  Box3Helper,
  BufferGeometry,
  Color,
  DirectionalLight,
  Float32BufferAttribute,
  GridHelper,
  Group,
  HemisphereLight,
  LineBasicMaterial,
  LineSegments,
  type Material,
  Mesh,
  MeshBasicMaterial,
  type Object3D,
  PerspectiveCamera,
  Raycaster,
  Scene,
  SphereGeometry,
  Vector2,
  Vector3,
  WebGLRenderer,
} from "three";
import { OrbitControls } from "three/examples/jsm/controls/OrbitControls.js";
import { CSS2DObject, CSS2DRenderer } from "three/examples/jsm/renderers/CSS2DRenderer.js";
import type { ContainerModelInfo, Pose } from "../api/types.ts";
import { maxExtent, type LoadedModel } from "./loaders.ts";
import { modelLocalMatrix, poseMatrix } from "./placement.ts";

/** 場面に置く Container 1 つ(Anchor の木のノード) */
export interface SceneNode {
  identifier: string;
  /** 系の中の容器の鍵。写しなら null(選べない) */
  key: string | null;
  label: string;
  /** 根の Anchor から見た位置姿勢 */
  pose: Pose;
  parentIdentifier: string | null;
  /** モデル情報。モデルが無ければ null */
  model: ContainerModelInfo | null;
}

/** 外接箱がこれより大きいモデルは注意を出す [m](scale の書き忘れなど) */
export const LARGE_MODEL_METERS = 100;

export interface SceneCallbacks {
  onSelect: (key: string | null) => void;
  /** 注意(Container の識別子 → 文)が変わった */
  onWarnings: (warnings: Map<string, string[]>) => void;
  loadModel: (identifier: string, info: ContainerModelInfo) => Promise<LoadedModel>;
}

interface Entry {
  node: SceneNode;
  group: Group;
  axes: AxesHelper;
  label: CSS2DObject;
  /** モデルを入れる入れ物。行列は origin ∘ U ∘ S */
  slot: Group;
  /** 選ぶための見えない球 */
  pick: Mesh;
  /** いま入れている(か読み込み中の)モデルの鍵 */
  modelKey: string | null;
  /** モデルの外接箱が LARGE_MODEL_METERS を超える */
  large: boolean;
}

function modelKeyOf(info: ContainerModelInfo | null): string | null {
  return info ? JSON.stringify([info.sha256, info.format, info.color, info.scale, info.up_axis, info.origin]) : null;
}

function disposeTree(o: Object3D) {
  o.traverse((x) => {
    if (x instanceof Mesh || x instanceof LineSegments) {
      // 形と材質はモデルの使い回し(ModelCache)と共有なので捨てない。自分で作った物だけ捨てる
      if (x.userData.owned) {
        (x.geometry as BufferGeometry).dispose();
        ((Array.isArray(x.material) ? x.material : [x.material]) as Material[]).forEach((m) => m.dispose());
      }
    }
  });
}

/** CSS の色(変数)を読む。読めなければ fallback */
function cssColor(el: HTMLElement, name: string, fallback: string): Color {
  const v = getComputedStyle(el).getPropertyValue(name).trim();
  try {
    return new Color(v || fallback);
  } catch {
    return new Color(fallback);
  }
}

export class ModelScene {
  private readonly host: HTMLElement;
  private readonly cb: SceneCallbacks;
  private readonly renderer: WebGLRenderer;
  private readonly labels: CSS2DRenderer;
  private readonly scene = new Scene();
  private readonly camera = new PerspectiveCamera(50, 1, 0.01, 20000);
  private readonly controls: OrbitControls;
  private readonly root = new Group();
  private readonly links: LineSegments;
  private grid: GridHelper;
  private readonly highlight: Box3Helper;
  private readonly highlightBox = new Box3();
  private readonly entries = new Map<string, Entry>();
  private readonly warnings = new Map<string, string[]>();
  private readonly resize: ResizeObserver;
  private selectedKey: string | null = null;
  private axisSize = 0.1;
  /** 全体に合わせた直後(この時刻 [ms] まで)にモデルを読み終えたら、もう一度合わせる */
  private refitUntil = 0;
  private readonly accent: Color;
  private readonly onPointerDown: (e: PointerEvent) => void;
  private readonly onPointerUp: (e: PointerEvent) => void;

  constructor(host: HTMLElement, cb: SceneCallbacks) {
    this.host = host;
    this.cb = cb;
    this.accent = cssColor(host, "--accent", "#3ee6ff");

    this.renderer = new WebGLRenderer({ antialias: true, alpha: true });
    this.renderer.setPixelRatio(window.devicePixelRatio);
    this.renderer.domElement.className = "model-canvas";
    host.appendChild(this.renderer.domElement);
    this.labels = new CSS2DRenderer();
    this.labels.domElement.className = "model-labels";
    host.appendChild(this.labels.domElement);

    // juiz の座標は Z 上(container-model.md §4.1)
    this.camera.up.set(0, 0, 1);
    this.camera.position.set(2, -3, 2);
    this.controls = new OrbitControls(this.camera, this.renderer.domElement);
    this.controls.enableDamping = true;
    // ホイールでは指した所へ寄る(小さな部品を見やすく)
    this.controls.zoomToCursor = true;

    this.scene.add(new HemisphereLight(0xffffff, 0x334044, 1.6));
    this.scene.add(new AmbientLight(0xffffff, 0.3));
    const sun = new DirectionalLight(0xffffff, 1.8);
    sun.position.set(3, -2, 5);
    this.scene.add(sun);

    this.grid = this.makeGrid(1);
    this.scene.add(this.grid);
    this.scene.add(this.root);

    // Transform は親子を結ぶ線
    this.links = new LineSegments(
      new BufferGeometry(),
      new LineBasicMaterial({ color: cssColor(host, "--kind-container", "#3ee6ff"), transparent: true, opacity: 0.6 }),
    );
    this.links.userData.owned = true;
    this.scene.add(this.links);

    this.highlight = new Box3Helper(this.highlightBox, this.accent);
    this.highlight.visible = false;
    this.scene.add(this.highlight);

    // 少しでも動かしたら(回す・ずらす)選ばない
    let down: Vector2 | null = null;
    this.onPointerDown = (e) => {
      down = new Vector2(e.clientX, e.clientY);
    };
    this.onPointerUp = (e) => {
      if (!down || e.button !== 0) return;
      const moved = down.distanceTo(new Vector2(e.clientX, e.clientY));
      down = null;
      if (moved < 4) this.pickAt(e);
    };
    this.renderer.domElement.addEventListener("pointerdown", this.onPointerDown);
    this.renderer.domElement.addEventListener("pointerup", this.onPointerUp);

    this.resize = new ResizeObserver(() => this.setSize());
    this.resize.observe(host);
    this.setSize();
    this.renderer.setAnimationLoop(() => this.render());
  }

  private makeGrid(step: number): GridHelper {
    const g = new GridHelper(step * 20, 20, 0x2c4650, 0x16242a);
    // GridHelper は XZ 平面なので、XY 平面(Z 上)に倒す
    g.rotation.x = Math.PI / 2;
    g.userData.owned = true;
    return g;
  }

  private setSize() {
    const w = Math.max(1, this.host.clientWidth);
    const h = Math.max(1, this.host.clientHeight);
    this.renderer.setSize(w, h);
    this.labels.setSize(w, h);
    this.camera.aspect = w / h;
    this.camera.updateProjectionMatrix();
  }

  private render() {
    this.controls.update();
    if (this.highlight.visible) this.updateHighlight();
    this.renderer.render(this.scene, this.camera);
    this.labels.render(this.scene, this.camera);
  }

  private pickAt(e: PointerEvent) {
    const rect = this.renderer.domElement.getBoundingClientRect();
    const ndc = new Vector2(((e.clientX - rect.left) / rect.width) * 2 - 1, -((e.clientY - rect.top) / rect.height) * 2 + 1);
    const ray = new Raycaster();
    ray.setFromCamera(ndc, this.camera);
    const targets = [...this.entries.values()].flatMap((x) => [x.slot, x.pick]);
    for (const hit of ray.intersectObjects(targets, true)) {
      let o: Object3D | null = hit.object;
      while (o && o.userData.identifier === undefined) o = o.parent;
      const entry = o ? this.entries.get(o.userData.identifier as string) : undefined;
      if (entry?.node.key) {
        this.cb.onSelect(entry.node.key);
        return;
      }
    }
    this.cb.onSelect(null);
  }

  private createEntry(node: SceneNode): Entry {
    const group = new Group();
    group.matrixAutoUpdate = false;
    group.userData.identifier = node.identifier;
    const axes = new AxesHelper(1);
    // x 赤・y 緑・z 青(container-model.md §12。AxesHelper の既定は先が黄色寄りになる)
    axes.setColors(new Color(0xff4d4d), new Color(0x4dff4d), new Color(0x4d8dff));
    axes.scale.setScalar(this.axisSize);
    axes.userData.owned = true;
    group.add(axes);
    const slot = new Group();
    slot.matrixAutoUpdate = false;
    group.add(slot);
    const pick = new Mesh(new SphereGeometry(1, 8, 6), new MeshBasicMaterial({ visible: false }));
    pick.scale.setScalar(this.axisSize * 0.6);
    pick.userData.owned = true;
    group.add(pick);
    const div = document.createElement("div");
    div.className = "model-label";
    div.addEventListener("click", () => {
      const e = this.entries.get(node.identifier);
      if (e?.node.key) this.cb.onSelect(e.node.key);
    });
    const label = new CSS2DObject(div);
    label.center.set(-0.08, 1.2);
    group.add(label);
    this.root.add(group);
    return { node, group, axes, label, slot, pick, modelKey: null, large: false };
  }

  private removeEntry(e: Entry) {
    this.root.remove(e.group);
    e.label.element.remove();
    disposeTree(e.group);
    this.warnings.delete(e.node.identifier);
  }

  private setWarnings(identifier: string, list: string[]) {
    if (list.length === 0) {
      if (!this.warnings.delete(identifier)) return;
    } else {
      this.warnings.set(identifier, list);
    }
    this.cb.onWarnings(new Map(this.warnings));
  }

  /** モデルを入れ替える。読み終える前に別のモデルに変わったら、古い方は捨てる */
  private attachModel(e: Entry) {
    const info = e.node.model;
    const key = modelKeyOf(info);
    if (key === e.modelKey) return;
    e.modelKey = key;
    e.large = false;
    for (const c of [...e.slot.children]) {
      e.slot.remove(c);
      disposeTree(c);
    }
    e.axes.visible = true;
    if (!info) {
      this.setWarnings(e.node.identifier, []);
      return;
    }
    e.slot.matrix.copy(modelLocalMatrix(info));
    const identifier = e.node.identifier;
    this.cb
      .loadModel(identifier, info)
      .then((loaded) => {
        const cur = this.entries.get(identifier);
        if (cur !== e || e.modelKey !== key) return;
        const warnings = [...loaded.warnings];
        if (loaded.object) {
          e.slot.add(loaded.object);
          e.axes.visible = false;
          const meters = maxExtent(loaded.object) * (info.scale > 0 ? info.scale : 1);
          e.large = meters > LARGE_MODEL_METERS;
          if (e.large) {
            warnings.push(
              `外接箱が ${meters.toFixed(0)} m あります。単位が m でないなら conf の model.scale を確かめてください`,
            );
          }
        }
        this.setWarnings(identifier, warnings);
        if (loaded.object && performance.now() < this.refitUntil) this.fit(false);
      })
      .catch((err: unknown) => {
        if (this.entries.get(identifier) !== e || e.modelKey !== key) return;
        this.setWarnings(identifier, [`モデルを読めません: ${err instanceof Error ? err.message : String(err)}`]);
      });
  }

  /** Anchor の木を置き直す。1 回目(か Anchor を変えた後)は全体が見えるようにする */
  update(nodes: SceneNode[], fit = false) {
    const seen = new Set<string>();
    for (const node of nodes) {
      seen.add(node.identifier);
      let e = this.entries.get(node.identifier);
      if (!e) {
        e = this.createEntry(node);
        this.entries.set(node.identifier, e);
      }
      e.node = node;
      e.group.matrix.copy(poseMatrix(node.pose));
      e.group.matrixWorldNeedsUpdate = true;
      const el = e.label.element;
      el.textContent = node.label;
      el.title = node.identifier;
      el.classList.toggle("selectable", !!node.key);
      el.classList.toggle("selected", !!node.key && node.key === this.selectedKey);
      this.attachModel(e);
    }
    for (const [id, e] of [...this.entries]) {
      if (!seen.has(id)) {
        this.removeEntry(e);
        this.entries.delete(id);
      }
    }
    if (seen.size < this.entries.size) this.cb.onWarnings(new Map(this.warnings));

    // 親子を結ぶ線
    const pos: number[] = [];
    for (const n of nodes) {
      if (!n.parentIdentifier) continue;
      const a = n.pose.position;
      const b = this.entries.get(n.parentIdentifier)?.node.pose.position;
      if (b) pos.push(b.x, b.y, b.z, a.x, a.y, a.z);
    }
    this.links.geometry.setAttribute("position", new Float32BufferAttribute(pos, 3));
    this.links.geometry.computeBoundingSphere();
    this.updateHighlight();
    if (fit) this.fit();
  }

  setSelected(key: string | null) {
    this.selectedKey = key;
    for (const e of this.entries.values()) {
      e.label.element.classList.toggle("selected", !!key && e.node.key === key);
    }
    this.updateHighlight();
  }

  private updateHighlight() {
    const e = [...this.entries.values()].find((x) => !!this.selectedKey && x.node.key === this.selectedKey);
    this.highlight.visible = !!e;
    if (e) this.boxOf(e, this.highlightBox);
  }

  /** Container の見えている形(モデル、無ければ座標軸)の外接箱 */
  private boxOf(e: Entry, box: Box3): Box3 {
    e.group.updateMatrixWorld(true);
    return box.setFromObject(e.slot.children.length > 0 ? e.slot : e.axes);
  }

  /**
   * 全体(Container の位置とモデル)が見えるようにする。座標軸の大きさと格子の目も合わせる。
   * follow なら、この後 3 秒の間に読み終えたモデルにも合わせ直す(最初の表示と Anchor を変えたとき)
   */
  fit(follow = true) {
    if (follow) this.refitUntil = performance.now() + 3000;
    this.root.updateMatrixWorld(true);
    const box = new Box3();
    for (const e of this.entries.values()) {
      box.expandByPoint(new Vector3().setFromMatrixPosition(e.group.matrixWorld));
      // 極端に大きいモデル(scale の書き忘れなど、注意を出すもの)は合わせる範囲に入れない
      if (e.slot.children.length > 0 && !e.large) box.union(new Box3().setFromObject(e.slot));
    }
    if (box.isEmpty()) return;
    const center = box.getCenter(new Vector3());
    const radius = Math.max(box.getSize(new Vector3()).length() / 2, 0.25);

    this.axisSize = Math.min(Math.max(radius * 0.08, 0.03), 5);
    for (const e of this.entries.values()) {
      e.axes.scale.setScalar(this.axisSize);
      e.pick.scale.setScalar(this.axisSize * 0.6);
    }
    const step = 10 ** Math.floor(Math.log10(radius));
    this.scene.remove(this.grid);
    disposeTree(this.grid);
    this.grid = this.makeGrid(step);
    this.scene.add(this.grid);

    const dir = new Vector3(0.6, -0.9, 0.7).normalize();
    const dist = radius / Math.sin((this.camera.fov * Math.PI) / 360);
    this.camera.position.copy(center).addScaledVector(dir, dist * 1.1);
    this.camera.near = Math.max(dist / 1000, 0.001);
    this.camera.far = dist * 100;
    this.camera.updateProjectionMatrix();
    this.controls.target.copy(center);
    this.controls.update();
  }

  dispose() {
    this.renderer.setAnimationLoop(null);
    this.resize.disconnect();
    this.renderer.domElement.removeEventListener("pointerdown", this.onPointerDown);
    this.renderer.domElement.removeEventListener("pointerup", this.onPointerUp);
    for (const e of this.entries.values()) this.removeEntry(e);
    this.entries.clear();
    this.controls.dispose();
    disposeTree(this.scene);
    this.renderer.dispose();
    this.renderer.domElement.remove();
    this.labels.domElement.remove();
  }
}
