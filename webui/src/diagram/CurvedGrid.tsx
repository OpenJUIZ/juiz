import { useStore, useViewport } from "@xyflow/react";

/** 湾曲の強さ。画面の隅で中心から見て何割ほど内へ寄るか */
const BEND = 0.07;
/** 目盛りの数字を出す間隔(図の座標) */
const LABEL_STEP = 1000;

/** 画面上の点を、中心から離れるほど内へ寄せる(樽型の歪み) */
function bend(x: number, y: number, w: number, h: number): [number, number] {
  const cx = w / 2;
  const cy = h / 2;
  const R = Math.hypot(cx, cy) || 1;
  const u = (x - cx) / R;
  const v = (y - cy) / R;
  const f = 1 - BEND * (u * u + v * v);
  return [cx + (x - cx) * f, cy + (y - cy) * f];
}

/** 画面上で 60px 以上あく、1・2・5 刻みの間隔 */
function gridStep(zoom: number): number {
  const target = 60 / zoom;
  const p = 10 ** Math.floor(Math.log10(target));
  for (const m of [1, 2, 5, 10]) if (m * p >= target) return m * p;
  return 10 * p;
}

/**
 * 図の背景の、ゆるく湾曲した座標グリッドと目盛り(#111、前身ソフトの画面を手本にしたもの)。
 * 線は図の座標に合わせて動き、描くときに画面の中心から樽型に歪ませる。
 */
export function CurvedGrid() {
  const { x: vx, y: vy, zoom } = useViewport();
  const w = useStore((s) => s.width);
  const h = useStore((s) => s.height);
  if (w === 0 || h === 0) return null;

  const step = gridStep(zoom);
  const toScreenX = (gx: number) => gx * zoom + vx;
  const toScreenY = (gy: number) => gy * zoom + vy;
  const gx0 = Math.floor(-vx / zoom / step) * step;
  const gx1 = (w - vx) / zoom;
  const gy0 = Math.floor(-vy / zoom / step) * step;
  const gy1 = (h - vy) / zoom;
  const samples = 24;

  const lines: { d: string; major: boolean }[] = [];
  const labels: { x: number; y: number; text: string; anchor: "start" | "middle" }[] = [];
  const isMajor = (g: number) => Math.abs(g % LABEL_STEP) < 1e-6;

  for (let gx = gx0; gx <= gx1; gx += step) {
    const sx = toScreenX(gx);
    const pts: string[] = [];
    for (let i = 0; i <= samples; i++) {
      const [px, py] = bend(sx, (h * i) / samples, w, h);
      pts.push(`${px.toFixed(1)},${py.toFixed(1)}`);
    }
    lines.push({ d: `M ${pts.join(" L ")}`, major: isMajor(gx) });
    if (isMajor(gx)) {
      const [px, py] = bend(sx, 14, w, h);
      labels.push({ x: px, y: py, text: String(Math.round(gx)), anchor: "middle" });
    }
  }
  for (let gy = gy0; gy <= gy1; gy += step) {
    const sy = toScreenY(gy);
    const pts: string[] = [];
    for (let i = 0; i <= samples; i++) {
      const [px, py] = bend((w * i) / samples, sy, w, h);
      pts.push(`${px.toFixed(1)},${py.toFixed(1)}`);
    }
    lines.push({ d: `M ${pts.join(" L ")}`, major: isMajor(gy) });
    if (isMajor(gy)) {
      const [px, py] = bend(6, sy, w, h);
      labels.push({ x: px, y: py - 4, text: String(Math.round(gy)), anchor: "start" });
    }
  }

  return (
    <svg className="curved-grid" width={w} height={h} aria-hidden="true">
      {lines.map((l, i) => (
        <path key={i} d={l.d} className={l.major ? "major" : undefined} />
      ))}
      {labels.map((l, i) => (
        <text key={i} x={l.x} y={l.y} textAnchor={l.anchor}>
          {l.text}
        </text>
      ))}
    </svg>
  );
}
