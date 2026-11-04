// juiz の値を画面の言葉に直す(#111 の用語)。

/** 実行文脈の状態 */
export function ecStateLabel(state: string | undefined): string {
  switch (state) {
    case "STARTED":
      return "実行中";
    case "STOPPED":
      return "停止中";
    case undefined:
      return "不明";
    default:
      return state;
  }
}

/** 接続の種類 */
export function connectionTypeLabel(type: string): string {
  switch (type) {
    case "Push":
      return "押込";
    case "Pull":
      return "引込";
    default:
      return type;
  }
}

/** Anchor の種類 */
export function anchorKindLabel(kind: string): string {
  switch (kind) {
    case "periodic":
      return "周期更新";
    case "static":
      return "静的";
    default:
      return kind;
  }
}

/** System をまたぐ Transform の取り込みの状態(#416) */
export function remoteStateLabel(state: string): string {
  switch (state) {
    case "ok":
      return "良好";
    case "stale":
      return "途絶中";
    case "lost":
      return "切断";
    default:
      return state;
  }
}

/** 取り込みの状態の説明(ツールチップ用) */
export function remoteStateDescription(state: string): string {
  switch (state) {
    case "ok":
      return "別の系から位置を取り込めています";
    case "stale":
      return "取り込みに失敗しています。最後に取れた値を使っています";
    case "lost":
      return "取り込めない時間が timeout を超えたので、写しを消しました";
    default:
      return state;
  }
}

/** juiz_std.common.Time を画面の時刻(手元の時計の表記)にする */
export function formatTime(t: { sec: number; nanosec: number } | null | undefined): string {
  if (!t) return "—";
  const d = new Date(t.sec * 1000 + Math.floor(t.nanosec / 1e6));
  const pad = (n: number, w = 2) => String(n).padStart(w, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}.${pad(d.getMilliseconds(), 3)}`;
}

/** 値を JSON で短く出す。max 文字を超えたら末尾を … にする */
export function shortValue(v: unknown, max = 16): string {
  const s = formatValue(v);
  return s.length > max ? `${s.slice(0, max - 1)}…` : s;
}

export function formatValue(v: unknown): string {
  if (v === undefined) return "";
  try {
    return JSON.stringify(v);
  } catch {
    return String(v);
  }
}
