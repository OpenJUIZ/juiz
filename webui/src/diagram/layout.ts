// elkjs で左から右へ自動配置する。
import type { ELK as ElkInstance, ElkNode } from "elkjs/lib/elk.bundled.js";
import type { DiagramGraph } from "./graph.ts";

export interface XY {
  x: number;
  y: number;
}

// elkjs は大きいので、初めて配置するときに読み込む
let elk: Promise<ElkInstance> | null = null;
function getElk(): Promise<ElkInstance> {
  elk ??= import("elkjs/lib/elk.bundled.js").then(({ default: ELK }) => new ELK());
  return elk;
}

const LAYOUT_OPTIONS: Record<string, string> = {
  "elk.algorithm": "layered",
  "elk.direction": "RIGHT",
  "elk.spacing.nodeNode": "56",
  "elk.layered.spacing.nodeNodeBetweenLayers": "110",
  "elk.spacing.componentComponent": "80",
  "elk.layered.nodePlacement.strategy": "BRANDES_KOEPF",
};

/** 各要素の左上の座標を返す */
export async function layoutGraph(graph: Pick<DiagramGraph, "nodes" | "edges">): Promise<Record<string, XY>> {
  const root: ElkNode = {
    id: "root",
    layoutOptions: LAYOUT_OPTIONS,
    children: graph.nodes.map((n) => ({ id: n.id, width: n.width, height: n.height })),
    edges: graph.edges.map((e) => ({ id: e.id, sources: [e.source], targets: [e.target] })),
  };
  const result = await (await getElk()).layout(root);
  const positions: Record<string, XY> = {};
  for (const c of result.children ?? []) positions[c.id] = { x: c.x ?? 0, y: c.y ?? 0 };
  return positions;
}
