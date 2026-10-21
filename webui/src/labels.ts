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

/** 値を一行で見せる */
export function formatValue(v: unknown): string {
  if (v === undefined) return "";
  try {
    return JSON.stringify(v);
  } catch {
    return String(v);
  }
}
