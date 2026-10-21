import { Handle, Position, type Node, type NodeProps } from "@xyflow/react";
import { ecStateLabel } from "../labels.ts";
import { TERMS, type TermKey } from "../terms.ts";
import type { ArgInfo, DiagramNodeData, DiagramNodeKind } from "./graph.ts";

export type JuizNode = Node<DiagramNodeData, "juiz">;

const KIND_TERM: Record<DiagramNodeKind, TermKey> = {
  process: "process",
  containerProcess: "containerProcess",
  container: "container",
  ec: "ec",
  topic: "topic",
};

/** 実行文脈の「実行中」の枠の中心の高さ。ここから対象の処理へ矢印を出す */
const EC_RUNNING_Y = 77;

/** 図の要素。種類ごとの図形を描き、選ばれていれば照準の輪を重ねる */
export function JuizNodeView({ data, width, height, selected }: NodeProps<JuizNode>) {
  const w = width ?? 0;
  const h = height ?? 0;
  return (
    <div className={`dnode dnode-${data.kind}`} style={{ width: w, height: h }}>
      <Handle type="target" position={Position.Left} isConnectable={false} />
      {data.kind === "process" || data.kind === "containerProcess" ? (
        <ProcessShape data={data} w={w} h={h} />
      ) : data.kind === "ec" ? (
        <EcShape data={data} />
      ) : (
        <div className="dnode-box">
          <div className="dnode-name">{data.name}</div>
          {data.kind === "container" && <div className="dnode-type">{data.typeName}</div>}
        </div>
      )}
      <Handle
        type="source"
        position={Position.Right}
        isConnectable={false}
        style={data.kind === "ec" ? { top: EC_RUNNING_Y } : undefined}
      />
      {selected && <Reticle data={data} w={w} h={h} />}
    </div>
  );
}

function ProcessShape({ data, w, h }: { data: DiagramNodeData; w: number; h: number }) {
  return (
    <>
      <svg className="dnode-shape" width={w} height={h}>
        <ellipse cx={w / 2} cy={h / 2} rx={w / 2 - 2} ry={h / 2 - 2} />
      </svg>
      <div className="dnode-label">
        <div className="dnode-name">{data.name}</div>
        <div className="dnode-type">{data.typeName}</div>
      </div>
    </>
  );
}

function EcShape({ data }: { data: DiagramNodeData }) {
  const states = ["STOPPED", "STARTED"];
  return (
    <div className="dnode-box dnode-ec-box">
      <div className="dnode-ec-head">
        <span className="dnode-name">{data.name}</span>
        {data.rate !== undefined && <span className="dnode-type">{data.rate} Hz</span>}
      </div>
      {states.map((s) => (
        <div key={s} className={`dnode-ec-state${data.state === s ? " current" : ""}`}>
          {ecStateLabel(s)}
        </div>
      ))}
    </div>
  );
}

/** 角度(度、右が 0 で時計回り)の点 */
function polar(cx: number, cy: number, r: number, deg: number): [number, number] {
  const a = (deg * Math.PI) / 180;
  return [cx + r * Math.cos(a), cy + r * Math.sin(a)];
}

function arcPath(cx: number, cy: number, r: number, from: number, to: number): string {
  const [x0, y0] = polar(cx, cy, r, from);
  const [x1, y1] = polar(cx, cy, r, to);
  const large = Math.abs(to - from) > 180 ? 1 : 0;
  return `M ${x0} ${y0} A ${r} ${r} 0 ${large} 1 ${x1} ${y1}`;
}

/** 引数の弧を並べる範囲(左側) */
const ARG_SPAN: [number, number] = [128, 232];

/**
 * 選んだ要素に重ねる照準の輪(#111)。目盛り付きの輪、回る弧、引き出し線の先の「種類 "名前"」。
 * 処理なら輪の左に引数ごとの弧、右に出力の弧を出し、つながっているものを光らせる。
 */
function Reticle({ data, w, h }: { data: DiagramNodeData; w: number; h: number }) {
  const r = Math.max(w, h) / 2 + 30;
  const pad = 140;
  const size = 2 * (r + pad);
  const c = size / 2;
  const ticks = Array.from({ length: 36 }, (_, i) => i * 10);
  const [lx0, ly0] = polar(c, c, r, -50);
  const lx1 = lx0 + 26;
  const ly1 = ly0 - 26;
  const term = TERMS[KIND_TERM[data.kind]];
  const isProcess = data.kind === "process" || data.kind === "containerProcess";
  return (
    <svg
      className="reticle"
      width={size}
      height={size}
      style={{ left: w / 2 - c, top: h / 2 - c }}
      aria-hidden="true"
    >
      <circle className="reticle-ring" cx={c} cy={c} r={r} />
      <circle className="reticle-inner" cx={c} cy={c} r={r - 10} />
      {ticks.map((deg) => {
        const long = deg % 30 === 0;
        const [x0, y0] = polar(c, c, r, deg);
        const [x1, y1] = polar(c, c, r - (long ? 9 : 4), deg);
        return <line key={deg} className="reticle-tick" x1={x0} y1={y0} x2={x1} y2={y1} />;
      })}
      <g className="reticle-spin" style={{ transformOrigin: `${c}px ${c}px` }}>
        {[0, 120, 240].map((deg) => (
          <path key={deg} className="reticle-arc" d={arcPath(c, c, r + 9, deg, deg + 46)} />
        ))}
      </g>
      {isProcess && <ArgArcs args={data.args ?? []} outputConnected={!!data.outputConnected} c={c} r={r + 22} />}
      <polyline className="reticle-leader" points={`${lx0},${ly0} ${lx1},${ly1} ${lx1 + 120},${ly1}`} />
      <text className="reticle-caption" x={lx1 + 4} y={ly1 - 6}>
        {term.ja} "{data.name}"
      </text>
    </svg>
  );
}

function ArgArcs({ args, outputConnected, c, r }: { args: ArgInfo[]; outputConnected: boolean; c: number; r: number }) {
  const [from, to] = ARG_SPAN;
  const seg = args.length > 0 ? (to - from) / args.length : 0;
  return (
    <g>
      {args.map((a, i) => {
        const a0 = from + i * seg + 2;
        const a1 = from + (i + 1) * seg - 2;
        const [tx, ty] = polar(c, c, r + 12, (a0 + a1) / 2);
        return (
          <g key={a.name} className={`reticle-port${a.connected ? " connected" : ""}`}>
            <path d={arcPath(c, c, r, a0, a1)} />
            <text x={tx} y={ty} textAnchor="end" dominantBaseline="middle">
              {a.name}
              <tspan className="reticle-port-type"> {a.typeName}</tspan>
            </text>
          </g>
        );
      })}
      <g className={`reticle-port output${outputConnected ? " connected" : ""}`}>
        <path d={arcPath(c, c, r, -24, 24)} />
        <text x={c + r + 12} y={c} dominantBaseline="middle">
          出力
        </text>
      </g>
    </g>
  );
}
