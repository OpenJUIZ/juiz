// 手で動かした要素の位置をブラウザの localStorage に保存する(#115)。
// 接続先ごとに分け、要素の鍵(selection.ts)ごとに左上の座標を持つ。
import type { XY } from "./layout.ts";

export type SavedPositions = Record<string, XY>;

function storageKey(serverUrl: string): string {
  return `juiz-webui.positions:${serverUrl}`;
}

export function loadPositions(serverUrl: string): SavedPositions {
  try {
    const raw = window.localStorage.getItem(storageKey(serverUrl));
    if (!raw) return {};
    const v = JSON.parse(raw) as unknown;
    if (typeof v !== "object" || v === null) return {};
    const out: SavedPositions = {};
    for (const [k, p] of Object.entries(v as Record<string, unknown>)) {
      const xy = p as Partial<XY> | null;
      if (xy && Number.isFinite(xy.x) && Number.isFinite(xy.y)) out[k] = { x: xy.x!, y: xy.y! };
    }
    return out;
  } catch {
    return {};
  }
}

export function savePositions(serverUrl: string, positions: SavedPositions): void {
  try {
    if (Object.keys(positions).length === 0) {
      window.localStorage.removeItem(storageKey(serverUrl));
    } else {
      window.localStorage.setItem(storageKey(serverUrl), JSON.stringify(positions));
    }
  } catch {
    // 保存できない環境では今回だけの配置になる
  }
}
