import { BaseEdge, EdgeLabelRenderer, getBezierPath, type Edge, type EdgeProps } from "@xyflow/react";
import type { DiagramEdgeData } from "./graph.ts";

export type JuizEdge = Edge<DiagramEdgeData, "juiz">;

/**
 * 図の線。種類ごとの色と線種は diagram.css の .dedge-*。
 * 引数名は矢印の先に出す。道具リンク(容器 → 容器処理)は処理側の端を白丸にする。
 */
export function JuizEdgeView(props: EdgeProps<JuizEdge>) {
  const { id, sourceX, sourceY, targetX, targetY, sourcePosition, targetPosition, markerEnd, selected, data } = props;
  const kind = data?.kind ?? "push";
  const [path] = getBezierPath({ sourceX, sourceY, sourcePosition, targetX, targetY, targetPosition });
  return (
    <>
      <BaseEdge
        id={id}
        path={path}
        markerEnd={markerEnd}
        className={`dedge dedge-${kind}${selected ? " selected" : ""}`}
        interactionWidth={16}
      />
      {kind === "instrument" && <circle className="dedge-instrument-end" cx={targetX} cy={targetY} r={5} />}
      {data?.label && (
        <EdgeLabelRenderer>
          <div
            className={`dedge-label dedge-label-${kind}`}
            style={{ transform: `translate(-100%, -100%) translate(${targetX - 12}px, ${targetY - 2}px)` }}
          >
            {data.label}
          </div>
        </EdgeLabelRenderer>
      )}
    </>
  );
}
