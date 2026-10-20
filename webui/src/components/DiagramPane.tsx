import { Background, BackgroundVariant, ReactFlow, type Edge, type Node } from "@xyflow/react";

const nodes: Node[] = [];
const edges: Edge[] = [];

/** 中央の系の図。骨組みでは空の図面だけ置く。描画は #115 */
export function DiagramPane() {
  return (
    <main className="diagram-pane">
      <ReactFlow nodes={nodes} edges={edges} colorMode="dark" proOptions={{ hideAttribution: true }}>
        <Background
          variant={BackgroundVariant.Lines}
          gap={80}
          color="var(--grid)"
          bgColor="var(--bg)"
        />
      </ReactFlow>
    </main>
  );
}
