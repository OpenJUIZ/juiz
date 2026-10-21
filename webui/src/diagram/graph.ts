// 系のモデル(../model.ts)から、図に描く要素と線を組み立てる。
// 表記は #111 で決めたもの(処理は楕円、容器は長方形、容器処理は容器の外に置いて道具リンクで結ぶ、
// 実行文脈は状態付きの長方形、話題は長方形)。
// 副系があるときは、系ごとに破線枠でまとめ、別の系どうしの接続は枠をまたいで描く(#116)。

import type { ProcessElement, SystemModel } from "../model.ts";

export type DiagramNodeKind = "process" | "containerProcess" | "container" | "ec" | "topic";

/** 処理の引数。選択時の照準の輪に弧として出す */
export interface ArgInfo {
  name: string;
  typeName: string;
  /** 接続または話題の購読で値が入ってくる */
  connected: boolean;
}

export interface DiagramNodeData extends Record<string, unknown> {
  kind: DiagramNodeKind;
  name: string;
  typeName: string;
  /** 処理と容器処理の引数 */
  args?: ArgInfo[];
  /** 処理と容器処理の出力がどこかへつながっている */
  outputConnected?: boolean;
  /** 実行文脈の状態(STARTED / STOPPED) */
  state?: string;
  /** 実行文脈の周期 [Hz] */
  rate?: number;
}

export interface DiagramNode {
  /** 要素の鍵(Element.key)。選択にもそのまま使う */
  id: string;
  data: DiagramNodeData;
  width: number;
  height: number;
}

/**
 * 線の種類。
 * push / pull は処理どうしの接続、instrument は容器 → 容器処理(道具リンク)、
 * ec は実行文脈 → 対象の処理、publish は処理 → 話題、subscribe は話題 → 処理。
 */
export type DiagramEdgeKind = "push" | "pull" | "instrument" | "ec" | "publish" | "subscribe";

export interface DiagramEdgeData extends Record<string, unknown> {
  kind: DiagramEdgeKind;
  /** 矢印の先に出す引数名 */
  label?: string;
}

export interface DiagramEdge {
  id: string;
  source: string;
  target: string;
  data: DiagramEdgeData;
  /** 選べる線(接続)なら、その要素の鍵 */
  selectKey?: string;
}

/** 系の破線枠。中の要素の位置から大きさを決める */
export interface DiagramGroup {
  /** 系の鍵(SystemModel.key) */
  id: string;
  name: string;
  /** 「系」か「副系」か */
  isRoot: boolean;
  /** 副系へ経由する仲介の種類(http, qmp など) */
  via: string | null;
  error: string | null;
  /** 枠に入る要素の鍵 */
  members: string[];
}

export interface DiagramGraph {
  nodes: DiagramNode[];
  edges: DiagramEdge[];
  /** 副系が無ければ空(枠を描かない) */
  groups: DiagramGroup[];
  /** 要素と線の顔ぶれ。変わったときだけ配置し直す */
  signature: string;
}

export const EMPTY_GRAPH: DiagramGraph = { nodes: [], edges: [], groups: [], signature: "" };

/** 画面上の文字幅のおおよそ。全角は 1 文字 14px、半角は 8px で数える */
function textWidth(s: string): number {
  let w = 0;
  for (const ch of s) w += ch.charCodeAt(0) > 0xff ? 14 : 8;
  return w;
}

/** 系の枠の見出し(用語・名前・仲介・取得不可)が収まる幅 */
export function frameMinWidth(g: Pick<DiagramGroup, "name" | "via" | "error">): number {
  return 80 + textWidth(g.name) * 1.1 + (g.via ? textWidth(g.via) + 24 : 0) + (g.error ? 80 : 0);
}

/** 名前と型名(小さく出す)が収まる幅 */
function fitWidth(name: string, typeName: string, pad: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, Math.max(textWidth(name), textWidth(typeName) * 0.8) + pad));
}

/** 系とその副系を、親から順に並べる */
function allSystems(sys: SystemModel): SystemModel[] {
  return [sys, ...sys.subsystems.flatMap(allSystems)];
}

export function buildGraph(root: SystemModel | null): DiagramGraph {
  if (!root) return EMPTY_GRAPH;
  const nodes: DiagramNode[] = [];
  const edges: DiagramEdge[] = [];
  const groups: DiagramGroup[] = [];
  const systems = allSystems(root);
  for (const sys of systems) {
    const first = nodes.length;
    addSystem(sys, nodes, edges);
    groups.push({
      id: sys.key,
      name: sys.name,
      isRoot: sys === root,
      via: sys.via?.typeName ?? null,
      error: sys.error,
      members: nodes.slice(first).map((n) => n.id),
    });
  }
  // 線の端が図に無いもの(古い juiz で副系の要素を指す接続など)は描かない
  const ids = new Set(nodes.map((n) => n.id));
  const shown = edges.filter((e) => ids.has(e.source) && ids.has(e.target));
  const shownGroups = systems.length > 1 ? groups : [];
  const signature = [
    ...nodes.map((n) => n.id).sort(),
    "|",
    ...shown.map((e) => e.id).sort(),
    "|",
    ...shownGroups.map((g) => g.id).sort(),
  ].join("\n");
  return { nodes, edges: shown, groups: shownGroups, signature };
}

function addSystem(sys: SystemModel, nodes: DiagramNode[], edges: DiagramEdge[]): void {

  const processNode = (p: ProcessElement): DiagramNode => {
    const subscribed = new Set(Object.keys(p.profile.subscribes ?? {}));
    const inputs = new Set(p.inputs.map((c) => c.profile.arg_name));
    return {
      id: p.key,
      data: {
        kind: p.kind,
        name: p.name,
        typeName: p.typeName,
        args: (p.profile.arguments ?? []).map((a) => ({
          name: a.name,
          typeName: a.type_name,
          connected: inputs.has(a.name) || subscribed.has(a.name),
        })),
        outputConnected: p.outputs.length > 0 || (p.profile.publishes ?? []).length > 0,
      },
      width: fitWidth(p.name, p.typeName, 64, 132, 280),
      height: 64,
    };
  };

  for (const p of sys.processes) nodes.push(processNode(p));
  for (const p of sys.containerProcesses) nodes.push(processNode(p));

  for (const c of sys.containers) {
    nodes.push({
      id: c.key,
      data: { kind: "container", name: c.name, typeName: c.typeName },
      width: fitWidth(c.name, c.typeName, 40, 140, 280),
      height: 60,
    });
    // 容器処理は容器の外に置き、容器から道具リンクで結ぶ
    for (const p of c.processes) {
      edges.push({ id: `instrument|${c.key}|${p.key}`, source: c.key, target: p.key, data: { kind: "instrument" } });
    }
  }

  for (const ec of sys.ecs) {
    nodes.push({
      id: ec.key,
      data: { kind: "ec", name: ec.name, typeName: ec.typeName, state: ec.profile.state, rate: ec.profile.rate },
      width: fitWidth(ec.name, ec.typeName, 40, 150, 260),
      height: 96,
    });
    for (const t of ec.targets) {
      if (t.key) edges.push({ id: `ec|${ec.key}|${t.key}`, source: ec.key, target: t.key, data: { kind: "ec" } });
    }
  }

  for (const t of sys.topics) {
    nodes.push({
      id: t.key,
      data: { kind: "topic", name: t.name, typeName: t.typeName },
      width: fitWidth(t.name, "", 40, 110, 240),
      height: 44,
    });
    for (const p of t.publishers) {
      edges.push({ id: `publish|${p.key}|${t.key}`, source: p.key, target: t.key, data: { kind: "publish" } });
    }
    for (const s of t.subscribers) {
      edges.push({
        id: `subscribe|${t.key}|${s.argName}|${s.process.key}`,
        source: t.key,
        target: s.process.key,
        data: { kind: "subscribe", label: s.argName },
      });
    }
  }

  for (const c of sys.connections ?? []) {
    // 相手の処理が見つからない接続は描かない
    if (!c.sourceKey || !c.destinationKey) continue;
    edges.push({
      id: c.key,
      source: c.sourceKey,
      target: c.destinationKey,
      data: { kind: c.profile.connection_type === "Pull" ? "pull" : "push", label: c.profile.arg_name },
      selectKey: c.key,
    });
  }

}
