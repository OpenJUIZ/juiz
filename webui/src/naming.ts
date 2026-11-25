// この機械の名前の一覧(docs/spec/naming.md §9.4、#618)から、ほかの系の管理画面を開く住所を作る。
import type { NameRegistration } from "./api/types.ts";

/** 登録名(ネームスペース/名前、ルートなら名前だけ) */
export function registeredName(r: NameRegistration): string {
  const ns = r.namespace.replace(/^\/+|\/+$/g, "");
  return ns ? `${ns}/${r.name}` : r.name;
}

/** 登録の並べ方。登録名の順 */
export function sortRegistrations(list: NameRegistration[]): NameRegistration[] {
  return [...list].sort((a, b) => registeredName(a).localeCompare(registeredName(b)));
}

/**
 * その系の管理画面の住所(`http://<host>:<port>/ui/`)。HTTP の仲介が無ければ null。
 * 待ち受けが 0.0.0.0・127.0.0.1 などなら、いま一覧を取った juiz の host(baseUrl、空なら画面のオリジン)を使う。
 * 一覧の系は一覧を取った juiz と同じ機械にいるので、同じ host で届く
 */
export function webUiUrl(r: NameRegistration, baseUrl: string, origin: string): string | null {
  const http = r.brokers.find((b) => b.type_name === "http");
  if (!http) return null;
  const m = /^http:\/\/(.*):(\d+)\/?$/.exec(http.id);
  if (!m) return null;
  let host = m[1] ?? "";
  // ループバックも置き換える(別のマシンからこの画面を開いたとき、127.0.0.1 ではそこへ届かないため)
  if (["", "0.0.0.0", "::", "[::]", "127.0.0.1", "localhost", "[::1]", "::1"].includes(host)) {
    try {
      host = new URL(baseUrl || origin).hostname;
    } catch {
      host = "127.0.0.1";
    }
    if (host.includes(":") && !host.startsWith("[")) host = `[${host}]`;
  }
  return `http://${host}:${m[2] ?? ""}/ui/`;
}
