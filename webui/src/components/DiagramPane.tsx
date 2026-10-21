import {
  MarkerType,
  Panel,
  ReactFlow,
  ReactFlowProvider,
  useReactFlow,
  type NodeChange,
} from "@xyflow/react";
import { useCallback, useEffect, useMemo, useState } from "react";
import { CurvedGrid } from "../diagram/CurvedGrid.tsx";
import { JuizEdgeView, type JuizEdge } from "../diagram/edges.tsx";
import { buildGraph, type DiagramEdgeKind, type DiagramGraph } from "../diagram/graph.ts";
import { layoutGraph, type XY } from "../diagram/layout.ts";
import { JuizNodeView, type JuizNode } from "../diagram/nodes.tsx";
import { loadPositions, savePositions, type SavedPositions } from "../diagram/positions.ts";
import "../diagram/diagram.css";
import type { Model } from "../model.ts";

interface Props {
  model: Model | null;
  /** 位置の保存先を接続先ごとに分けるのに使う */
  serverUrl: string;
  selectedKey: string | null;
  onSelect: (key: string | null) => void;
}

const nodeTypes = { juiz: JuizNodeView };
const edgeTypes = { juiz: JuizEdgeView };

/** 矢印の色(diagram.css の線の色と合わせる)。道具リンクは矢印なしで白丸 */
const MARKER_COLOR: Record<DiagramEdgeKind, string | null> = {
  push: "var(--kind-connection)",
  pull: "var(--kind-connection)",
  instrument: null,
  ec: "var(--kind-ec)",
  publish: "var(--kind-publish)",
  subscribe: "var(--kind-topic)",
};

/** 中央の系の図(#115) */
export function DiagramPane(props: Props) {
  return (
    <main className="diagram-pane">
      <ReactFlowProvider>
        <Diagram {...props} />
      </ReactFlowProvider>
    </main>
  );
}

function Diagram({ model, serverUrl, selectedKey, onSelect }: Props) {
  const { fitView } = useReactFlow();
  const graph = useMemo(() => buildGraph(model?.root ?? null), [model]);

  // 要素と線の顔ぶれが変わったときだけ配置し直す。ポーリングのたびに配置が動かないようにする
  const [structure, setStructure] = useState<DiagramGraph>(graph);
  if (structure.signature !== graph.signature) setStructure(graph);

  const [auto, setAuto] = useState<{ signature: string; positions: Record<string, XY> } | null>(null);
  useEffect(() => {
    let cancelled = false;
    layoutGraph(structure)
      .then((positions) => {
        if (!cancelled) setAuto({ signature: structure.signature, positions });
      })
      .catch((e: unknown) => console.warn("自動配置に失敗しました", e));
    return () => {
      cancelled = true;
    };
  }, [structure]);

  // 初めて配置できたときと、配置し直したときに全体が見えるようにする
  useEffect(() => {
    if (!auto) return;
    const id = window.requestAnimationFrame(() => void fitView({ padding: 0.25, duration: 300 }));
    return () => window.cancelAnimationFrame(id);
  }, [auto, fitView]);

  // 手で動かした位置。接続先ごとに localStorage に保存する
  const [saved, setSaved] = useState<{ serverUrl: string; positions: SavedPositions }>(() => ({
    serverUrl,
    positions: loadPositions(serverUrl),
  }));
  if (saved.serverUrl !== serverUrl) setSaved({ serverUrl, positions: loadPositions(serverUrl) });

  const nodes = useMemo<JuizNode[]>(
    () =>
      graph.nodes.map((n) => ({
        id: n.id,
        type: "juiz",
        position: saved.positions[n.id] ?? auto?.positions[n.id] ?? { x: 0, y: 0 },
        data: n.data,
        width: n.width,
        height: n.height,
        selected: n.id === selectedKey,
        // 照準の輪が他の要素の下に隠れないようにする
        zIndex: n.id === selectedKey ? 10 : 0,
        // 配置が済むまでは出さない(左上に重なって見えるのを避ける)
        hidden: !saved.positions[n.id] && !auto?.positions[n.id],
      })),
    [graph, saved, auto, selectedKey],
  );

  const edges = useMemo<JuizEdge[]>(
    () =>
      graph.edges.map((e) => {
        const color = MARKER_COLOR[e.data.kind];
        return {
          id: e.id,
          type: "juiz",
          source: e.source,
          target: e.target,
          data: e.data,
          selectable: !!e.selectKey,
          selected: !!e.selectKey && e.selectKey === selectedKey,
          markerEnd: color ? { type: MarkerType.ArrowClosed, color, width: 16, height: 16 } : undefined,
        };
      }),
    [graph, selectedKey],
  );

  const onNodesChange = useCallback(
    (changes: NodeChange<JuizNode>[]) => {
      const moved: SavedPositions = {};
      let dropped = false;
      for (const c of changes) {
        if (c.type !== "position") continue;
        if (c.position) moved[c.id] = c.position;
        if (c.dragging === false) dropped = true;
      }
      if (Object.keys(moved).length === 0 && !dropped) return;
      const positions = { ...saved.positions, ...moved };
      setSaved({ serverUrl: saved.serverUrl, positions });
      // 動かし終えたときだけ保存する
      if (dropped) savePositions(saved.serverUrl, positions);
    },
    [saved],
  );

  const resetLayout = useCallback(() => {
    savePositions(serverUrl, {});
    setSaved({ serverUrl, positions: {} });
    void fitView({ padding: 0.25, duration: 300 });
  }, [serverUrl, fitView]);

  return (
    <ReactFlow<JuizNode, JuizEdge>
      nodes={nodes}
      edges={edges}
      nodeTypes={nodeTypes}
      edgeTypes={edgeTypes}
      onNodesChange={onNodesChange}
      onNodeClick={(_, n) => onSelect(n.id)}
      onEdgeClick={(_, e) => {
        const key = graph.edges.find((g) => g.id === e.id)?.selectKey;
        if (key) onSelect(key);
      }}
      onPaneClick={() => onSelect(null)}
      nodesConnectable={false}
      elementsSelectable={false}
      minZoom={0.1}
      maxZoom={3}
      colorMode="dark"
      proOptions={{ hideAttribution: true }}
    >
      <CurvedGrid />
      <div className="diagram-vignette" />
      <Panel position="top-right" className="diagram-tools">
        <button type="button" onClick={resetLayout} title="手で動かした位置を消して自動配置に戻す">
          自動配置
        </button>
        <button type="button" onClick={() => void fitView({ padding: 0.25, duration: 300 })}>
          全体
        </button>
      </Panel>
      <Panel position="bottom-left" className="diagram-legend">
        <Legend />
      </Panel>
    </ReactFlow>
  );
}

const LEGEND: { kind: DiagramEdgeKind; label: string }[] = [
  { kind: "push", label: "押込" },
  { kind: "pull", label: "引込" },
  { kind: "ec", label: "実行" },
  { kind: "publish", label: "発行" },
  { kind: "subscribe", label: "購読" },
  { kind: "instrument", label: "容器" },
];

/** 線の種類の凡例 */
function Legend() {
  return (
    <ul>
      {LEGEND.map((l) => (
        <li key={l.kind}>
          <svg width="28" height="10" aria-hidden="true">
            <line x1="1" y1="5" x2="27" y2="5" className={`dedge dedge-${l.kind}`} />
          </svg>
          {l.label}
        </li>
      ))}
    </ul>
  );
}
