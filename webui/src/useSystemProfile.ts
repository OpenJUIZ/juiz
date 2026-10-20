import { useCallback, useEffect, useRef, useState } from "react";
import { JuizClient } from "./api/client.ts";
import type { SystemProfile } from "./api/types.ts";

export interface SystemProfileState {
  profile: SystemProfile | null;
  error: string | null;
  updatedAt: Date | null;
  refresh: () => void;
}

/** system/profile_full を取得する。intervalMs が null なら手動更新だけ */
export function useSystemProfile(serverUrl: string, intervalMs: number | null): SystemProfileState {
  const [profile, setProfile] = useState<SystemProfile | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [updatedAt, setUpdatedAt] = useState<Date | null>(null);
  const abortRef = useRef<AbortController | null>(null);

  const refresh = useCallback(() => {
    abortRef.current?.abort();
    const ac = new AbortController();
    abortRef.current = ac;
    new JuizClient(serverUrl)
      .systemProfileFull(ac.signal)
      .then((p) => {
        setProfile(p);
        setError(null);
        setUpdatedAt(new Date());
      })
      .catch((e: unknown) => {
        if (e instanceof DOMException && e.name === "AbortError") return;
        setError(e instanceof Error ? e.message : String(e));
      });
  }, [serverUrl]);

  useEffect(() => {
    refresh();
    if (intervalMs === null) return () => abortRef.current?.abort();
    const id = window.setInterval(refresh, intervalMs);
    return () => {
      window.clearInterval(id);
      abortRef.current?.abort();
    };
  }, [refresh, intervalMs]);

  return { profile, error, updatedAt, refresh };
}
