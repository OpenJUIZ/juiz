import type { paths } from "./openapi.gen.ts";
import type { ConnectionProfile, SystemProfile } from "./types.ts";

/** GET で呼べる API のパス(OpenAPI 定義から生成) */
type GetPath = {
  [P in keyof paths]: paths[P] extends { get: object } ? P : never;
}[keyof paths];

type QueryOf<P extends GetPath> = paths[P]["get"] extends {
  parameters: { query?: infer Q };
}
  ? Q
  : never;

export class JuizApiError extends Error {
  readonly status: number | undefined;

  constructor(message: string, status?: number) {
    super(message);
    this.name = "JuizApiError";
    this.status = status;
  }
}

/**
 * juiz の HTTP Broker を呼ぶクライアント。
 *
 * baseUrl が空なら画面と同じオリジン(開発サーバーの中継、または juiz からの配信)を使う。
 * 別オリジンの juiz を指すと CORS が要る(#112)。
 */
export class JuizClient {
  readonly baseUrl: string;

  constructor(baseUrl: string) {
    this.baseUrl = baseUrl.replace(/\/+$/, "");
  }

  async get<P extends GetPath>(path: P, query?: QueryOf<P>, signal?: AbortSignal): Promise<unknown> {
    const params = new URLSearchParams();
    for (const [k, v] of Object.entries((query ?? {}) as Record<string, unknown>)) {
      if (v !== undefined && v !== null) params.set(k, String(v));
    }
    const qs = params.size > 0 ? `?${params}` : "";
    let res: Response;
    try {
      res = await fetch(`${this.baseUrl}${path}${qs}`, { signal });
    } catch (e) {
      if (e instanceof DOMException && e.name === "AbortError") throw e;
      throw new JuizApiError(`接続できません: ${(e as Error).message}`);
    }
    if (!res.ok) {
      throw new JuizApiError(`${path} が ${res.status} を返しました`, res.status);
    }
    return res.json();
  }

  /** recursive なら副系の中身と各系の接続も入れてもらう(#116) */
  async systemProfileFull(recursive: boolean, signal?: AbortSignal): Promise<SystemProfile> {
    const query = recursive ? { recursive: "true" } : undefined;
    return (await this.get("/api/system/profile_full", query, signal)) as SystemProfile;
  }

  /** 接続の識別子の一覧 */
  async connectionList(signal?: AbortSignal): Promise<string[]> {
    return (await this.get("/api/connection/list", undefined, signal)) as string[];
  }

  async connectionProfileFull(identifier: string, signal?: AbortSignal): Promise<ConnectionProfile> {
    return (await this.get("/api/connection/profile_full", { identifier }, signal)) as ConnectionProfile;
  }
}
