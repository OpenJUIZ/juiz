import { useCallback, useEffect, useRef, useState } from "react";
import { JuizClient } from "./api/client.ts";
import { buildModel, type Model } from "./model.ts";

export interface SystemModelState {
  /** 最後に取れた系の情報。取得に失敗しても前回の分を残す */
  model: Model | null;
  error: string | null;
  updatedAt: Date | null;
  refresh: () => void;
}

/** 話題の発行・購読の接続か。識別子の両端のどちらかが "名前::topic" になっている */
function isTopicConnection(id: string): boolean {
  return id.split("|").some((end) => end.endsWith("::topic"));
}

interface Fetched {
  serverUrl: string;
  model: Model | null;
  error: string | null;
  updatedAt: Date | null;
}

/**
 * system/profile_full(副系の中身つき)を取得して画面用の形にする。
 * intervalMs が null なら手動更新だけ。
 */
export function useSystemModel(serverUrl: string, intervalMs: number | null): SystemModelState {
  const [fetched, setFetched] = useState<Fetched | null>(null);
  const abortRef = useRef<AbortController | null>(null);

  const refresh = useCallback(() => {
    abortRef.current?.abort();
    const ac = new AbortController();
    abortRef.current = ac;
    const client = new JuizClient(serverUrl);
    client
      .systemProfileFull(true, ac.signal)
      .then(async (profile) => {
        const url = client.baseUrl || window.location.origin;
        // recursive=true に対応した juiz なら、副系の中身と各系の接続が profile に入っている(#116)
        if (profile.connections) return buildModel(profile, url);
        // 古い juiz では接続が入っていないので、最上位の系の分だけ一つずつ取る。
        // 話題への発行・購読も接続として返るが、発行側は profile_full が 500 になるうえ、
        // 処理の publishes / subscribes で分かるので取らない。
        const ids = await client.connectionList(ac.signal);
        const results = await Promise.allSettled(
          ids
            .filter((id) => !isTopicConnection(id))
            .map((id) => client.connectionProfileFull(id, ac.signal).then((c) => ({ ...c, identifier: id }))),
        );
        if (ac.signal.aborted) throw new DOMException("aborted", "AbortError");
        const connections = results.flatMap((r) => (r.status === "fulfilled" ? [r.value] : []));
        return buildModel(profile, url, connections);
      })
      .then((model) => {
        setFetched({ serverUrl, model, error: null, updatedAt: new Date() });
      })
      .catch((e: unknown) => {
        if (e instanceof DOMException && e.name === "AbortError") return;
        const error = e instanceof Error ? e.message : String(e);
        setFetched((prev) =>
          prev?.serverUrl === serverUrl
            ? { ...prev, error }
            : { serverUrl, model: null, error, updatedAt: null },
        );
      })
      .finally(() => {
        if (abortRef.current === ac) abortRef.current = null;
      });
  }, [serverUrl]);

  useEffect(() => {
    refresh();
    if (intervalMs === null) return () => abortRef.current?.abort();
    // 前回の取得が終わっていなければ、その回は飛ばす(遅い juiz で取得が打ち切られ続けないように)
    const id = window.setInterval(() => {
      if (abortRef.current === null) refresh();
    }, intervalMs);
    return () => {
      window.clearInterval(id);
      abortRef.current?.abort();
    };
  }, [refresh, intervalMs]);

  // 接続先を変えたら、前の接続先の情報は出さない
  const cur = fetched?.serverUrl === serverUrl ? fetched : null;
  return {
    model: cur?.model ?? null,
    error: cur?.error ?? null,
    updatedAt: cur?.updatedAt ?? null,
    refresh,
  };
}
