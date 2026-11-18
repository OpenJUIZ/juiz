import { Handle, Position, type Node, type NodeProps } from "@xyflow/react";
import { useEffect, useState, type CSSProperties } from "react";
import { ecStateLabel, formatValue, shortValue } from "../labels.ts";
import { TERMS, type TermKey } from "../terms.ts";
import { RETICLE_CLOSE_MS, prefersReducedMotion } from "./motion.ts";
import { TOOL_HANDLE, type ArgInfo, type DiagramNodeData, type DiagramNodeKind } from "./graph.ts";

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

/**
 * 照準の輪を出すかどうか。選ばれていれば "open"、選択が外れてから閉じるアニメーションの間は "closing"、
 * それ以外は null(輪を外す)
 */
function useReticlePresence(selected: boolean): "open" | "closing" | null {
  const [wasSelected, setWasSelected] = useState(selected);
  const [closing, setClosing] = useState(false);
  if (selected !== wasSelected) {
    setWasSelected(selected);
    setClosing(!selected && !prefersReducedMotion());
  }
  useEffect(() => {
    if (!closing) return;
    const id = window.setTimeout(() => setClosing(false), RETICLE_CLOSE_MS);
    return () => window.clearTimeout(id);
  }, [closing]);
  return selected ? "open" : closing ? "closing" : null;
}

/** 図の要素。種類ごとの図形を描き、選ばれていれば照準の輪を重ねる */
export function JuizNodeView({ data, width, height, selected }: NodeProps<JuizNode>) {
  const w = width ?? 0;
  const h = height ?? 0;
  const reticle = useReticlePresence(!!selected);
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
      {data.kind === "containerProcess" && (
        <>
          {/* 容器からの道具リンクは楕円の上か下につなぐ。左端は引数の入り口と紛れるので避ける */}
          <Handle id={TOOL_HANDLE.top} type="target" position={Position.Top} isConnectable={false} />
          <Handle id={TOOL_HANDLE.bottom} type="target" position={Position.Bottom} isConnectable={false} />
        </>
      )}
      <Handle
        type="source"
        position={Position.Right}
        isConnectable={false}
        style={data.kind === "ec" ? { top: EC_RUNNING_Y } : undefined}
      />
      {reticle && <Reticle data={data} w={w} h={h} closing={reticle === "closing"} />}
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

/** 引き出し線の付け根の角度。輪はここから反時計回りに描き始める */
const LEADER_DEG = -50;

/** 目盛りを出し始める時刻と、1 本ずつずらす間隔(ms)。diagram.css の輪を描くアニメーションの後に続ける */
const TICKS_DELAY_MS = 380;
const TICK_STEP_MS = 6;

/**
 * 線を端から描くアニメーション用の破線の設定。stroke-dashoffset を --draw-length(負)から 0 へ動かすと、
 * 線の終わりから始まりへ向かって(弧なら反時計回りに)描かれる(diagram.css の reticle-ring-draw)。
 * 継ぎ目に隙間が出ないよう 1 だけ長めにとる
 */
function drawStyle(length: number): CSSProperties {
  const len = length + 1;
  return { strokeDasharray: `${len} ${len}`, ["--draw-length" as string]: `${-len}` };
}

/** 引数の弧を並べる範囲(左側) */
const ARG_SPAN: [number, number] = [128, 232];

/**
 * 選んだ要素に重ねる照準の輪(#111)。目盛り付きの輪、回る弧、引き出し線の先の「種類 "名前"」。
 * 処理なら輪の左に引数ごとの弧、右に出力の弧を出し、未接続・接続あり・拘束中を弧の塗りで分ける。
 * 開くときは、まず輪と回る弧が反時計回りに描かれて広がり、そのあと引き出し線・名前・目盛り・引数の弧を出す。
 * 閉じるときは広がりながら消える(diagram.css)。
 */
function Reticle({ data, w, h, closing }: { data: DiagramNodeData; w: number; h: number; closing: boolean }) {
  const r = Math.max(w, h) / 2 + 30;
  const pad = 140;
  const size = 2 * (r + pad);
  const c = size / 2;
  const ticks = Array.from({ length: 36 }, (_, i) => i * 10);
  const [lx0, ly0] = polar(c, c, r, LEADER_DEG);
  const lx1 = lx0 + 26;
  const ly1 = ly0 - 26;
  const term = TERMS[KIND_TERM[data.kind]];
  const isProcess = data.kind === "process" || data.kind === "containerProcess";
  // 引き出し線の長さ。伸びるアニメーションの破線の長さに使う
  const leaderLength = Math.hypot(lx1 - lx0, ly1 - ly0) + 120;
  return (
    <svg
      className={`reticle${closing ? " closing" : ""}`}
      width={size}
      height={size}
      style={{ left: w / 2 - c, top: h / 2 - c }}
      aria-hidden="true"
    >
      {/* 引き出し線の付け根から反時計回りに描く。円の描き始めは右(0 度)なので、付け根の角度まで回しておく */}
      <circle
        className="reticle-ring"
        cx={c}
        cy={c}
        r={r}
        transform={`rotate(${LEADER_DEG} ${c} ${c})`}
        style={drawStyle(2 * Math.PI * r)}
      />
      <circle className="reticle-inner" cx={c} cy={c} r={r - 10} />
      {ticks.map((deg) => {
        const long = deg % 30 === 0;
        const [x0, y0] = polar(c, c, r, deg);
        const [x1, y1] = polar(c, c, r - (long ? 9 : 4), deg);
        // 付け根から反時計回りの順に出す
        const order = ((((LEADER_DEG - deg) % 360) + 360) % 360) / 10;
        return (
          <line
            key={deg}
            className="reticle-tick"
            x1={x0}
            y1={y0}
            x2={x1}
            y2={y1}
            style={{ animationDelay: `${TICKS_DELAY_MS + order * TICK_STEP_MS}ms` }}
          />
        );
      })}
      <g className="reticle-spin" style={{ transformOrigin: `${c}px ${c}px` }}>
        {[0, 120, 240].map((deg) => (
          <path
            key={deg}
            className="reticle-arc"
            d={arcPath(c, c, r + 9, deg, deg + 46)}
            style={drawStyle((2 * Math.PI * (r + 9) * 46) / 360)}
          />
        ))}
      </g>
      {isProcess && <ArgArcs args={data.args ?? []} outputConnected={!!data.outputConnected} c={c} r={r + 22} />}
      <polyline
        className="reticle-leader"
        points={`${lx0},${ly0} ${lx1},${ly1} ${lx1 + 120},${ly1}`}
        style={{ strokeDasharray: leaderLength, ["--leader-length" as string]: `${leaderLength}` }}
      />
      <text className="reticle-caption" x={lx1 + 4} y={ly1 - 6}>
        {term.ja} "{data.name}"
      </text>
    </svg>
  );
}

/** 弧の塗り分け。接続ありを優先し、未接続で値があれば拘束中 */
function portState(a: ArgInfo): string {
  if (a.connected) return " connected";
  if (a.bound) return " bound";
  return "";
}

function ArgArcs({ args, outputConnected, c, r }: { args: ArgInfo[]; outputConnected: boolean; c: number; r: number }) {
  const [from, to] = ARG_SPAN;
  const seg = args.length > 0 ? (to - from) / args.length : 0;
  return (
    <g className="reticle-ports">
      {args.map((a, i) => {
        const a0 = from + i * seg + 2;
        const a1 = from + (i + 1) * seg - 2;
        const [tx, ty] = polar(c, c, r + 12, (a0 + a1) / 2);
        return (
          <g key={a.name} className={`reticle-port${portState(a)}`}>
            <path d={arcPath(c, c, r, a0, a1)} />
            <text x={tx} y={ty} textAnchor="end" dominantBaseline="middle">
              {a.name}
              <tspan className="reticle-port-type"> {a.typeName}</tspan>
            </text>
            {a.bound && (
              <text className="reticle-port-value" x={tx} y={ty + 14} textAnchor="end" dominantBaseline="middle">
                <title>拘束値 {formatValue(a.value)}</title>= {shortValue(a.value)}
              </text>
            )}
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

export interface SystemFrameData extends Record<string, unknown> {
  name: string;
  isRoot: boolean;
  via: string | null;
  error: string | null;
}

export type SystemFrameNode = Node<SystemFrameData, "systemFrame">;

/** 系の破線枠(#116)。見出しだけがクリックを受け、枠の中は下の要素や背景へ通す */
export function SystemFrameView({ data, width, height, selected }: NodeProps<SystemFrameNode>) {
  const term = TERMS[data.isRoot ? "system" : "subsystem"];
  return (
    <div
      className={`dframe${data.isRoot ? " root" : ""}${data.error ? " error" : ""}${selected ? " selected" : ""}`}
      style={{ width: width ?? 0, height: height ?? 0 }}
    >
      <div className="dframe-head" title={data.error ?? undefined}>
        <span className="dframe-term">{term.ja}</span>
        <span className="dframe-name">{data.name}</span>
        {data.via && <span className="dframe-via">{data.via}</span>}
        {data.error && <span className="dframe-error">取得不可</span>}
      </div>
    </div>
  );
}
