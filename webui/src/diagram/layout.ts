// elkjs で左から右へ自動配置する。
import type { ELK as ElkInstance, ElkNode } from "elkjs/lib/elk.bundled.js";
import { frameMinWidth, type DiagramGraph } from "./graph.ts";

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

/** 中に要素の無い系の枠の高さ */
export const EMPTY_FRAME_HEIGHT = 80;

/** 系の枠の内側の余白。上は見出しの分を広くとる */
export const GROUP_PADDING = { top: 44, side: 28, bottom: 28 };

/**
 * 各要素の左上の座標(図全体での位置)を返す。
 * 系の枠があれば、系ごとにまとめて配置し、別の系どうしの線も含めて並べる。
 */
export async function layoutGraph(
  graph: Pick<DiagramGraph, "nodes" | "edges" | "groups">,
): Promise<Record<string, XY>> {
  const leaf = (n: DiagramGraph["nodes"][number]): ElkNode => ({ id: n.id, width: n.width, height: n.height });
  const byId = new Map(graph.nodes.map((n) => [n.id, n]));
  const children: ElkNode[] =
    graph.groups.length === 0
      ? graph.nodes.map(leaf)
      : graph.groups.map((g) => ({
          id: `group:${g.id}`,
          layoutOptions: {
            ...LAYOUT_OPTIONS,
            "elk.padding": `[top=${GROUP_PADDING.top},left=${GROUP_PADDING.side},bottom=${GROUP_PADDING.bottom},right=${GROUP_PADDING.side}]`,
          },
          // 要素の無い系も枠だけは並べる。見出しが収まる幅はとる
          width: frameMinWidth(g),
          height: EMPTY_FRAME_HEIGHT,
          children: g.members.flatMap((id) => {
            const n = byId.get(id);
            return n ? [leaf(n)] : [];
          }),
        }));
  const root: ElkNode = {
    id: "root",
    layoutOptions: {
      ...LAYOUT_OPTIONS,
      // 枠をまたぐ線も含めて、枠の中の並びを決める
      "elk.hierarchyHandling": "INCLUDE_CHILDREN",
    },
    children,
    edges: graph.edges.map((e) => ({ id: e.id, sources: [e.source], targets: [e.target] })),
  };
  const result = await (await getElk()).layout(root);
  const positions: Record<string, XY> = {};
  // 枠の中の要素は枠からの相対位置で返るので、図全体の位置に直す
  const walk = (nodes: ElkNode[] | undefined, ox: number, oy: number) => {
    for (const c of nodes ?? []) {
      const x = ox + (c.x ?? 0);
      const y = oy + (c.y ?? 0);
      // 枠の位置も残しておく(中に要素の無い枠を置くのに使う)
      positions[c.id] = { x, y };
      if (c.children) walk(c.children, x, y);
    }
  };
  walk(result.children, 0, 0);
  return positions;
}
