import type { paths } from "./openapi.gen.ts";
import type {
  ConnectionProfile,
  ContainerModelInfo,
  PoseStamped,
  SystemProfile,
  TransformationsTreeNode,
  TransformProfile,
} from "./types.ts";

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

  /**
   * 処理・容器処理を呼ぶ(PATCH /api/process/call など、#197)。
   * 戻り値が画像なら Blob、それ以外は JSON の値。エラーなら応答の message を持つ JuizApiError
   */
  async call(
    kind: "process" | "container_process",
    identifier: string,
    args: Record<string, unknown>,
    signal?: AbortSignal,
  ): Promise<{ value: unknown } | { image: Blob }> {
    const path = kind === "process" ? "/api/process/call" : "/api/container_process/call";
    let res: Response;
    try {
      res = await fetch(`${this.baseUrl}${path}?${new URLSearchParams({ identifier })}`, {
        method: "PATCH",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(args),
        signal,
      });
    } catch (e) {
      if (e instanceof DOMException && e.name === "AbortError") throw e;
      throw new JuizApiError(`接続できません: ${(e as Error).message}`);
    }
    if (!res.ok) {
      let message = `${path} が ${res.status} を返しました`;
      try {
        const body = (await res.json()) as { message?: string };
        if (body.message) message = body.message;
      } catch {
        // 本文が JSON でなければ状態コードだけ
      }
      throw new JuizApiError(message, res.status);
    }
    if ((res.headers.get("Content-Type") ?? "").startsWith("image/")) {
      return { image: await res.blob() };
    }
    return { value: await res.json() };
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

  /* ---------- Transformations(#370、#373) ---------- */

  /** Transform の識別子の一覧(transform://<始点>|<終点>) */
  async transformList(signal?: AbortSignal): Promise<string[]> {
    return (await this.get("/api/transform/list", undefined, signal)) as string[];
  }

  /** Anchor の識別子の一覧。別の系から取り込んだ写しの Anchor も入る(#416) */
  async anchorList(signal?: AbortSignal): Promise<string[]> {
    return (await this.get("/api/anchor/list", undefined, signal)) as string[];
  }

  async transformProfileFull(identifier: string, signal?: AbortSignal): Promise<TransformProfile> {
    return (await this.get("/api/transform/profile_full", { identifier }, signal)) as TransformProfile;
  }

  /** Anchor を根とする木。各ノードに Anchor から見た位置姿勢と、親からの Transform の値が入る */
  async transformationsTree(anchor: string, signal?: AbortSignal): Promise<TransformationsTreeNode> {
    return (await this.get("/api/transformations/tree", { anchor }, signal)) as TransformationsTreeNode;
  }

  /* ---------- Container の 3D モデル(#381、#384) ---------- */

  /** モデル情報。モデルが無ければ null */
  async containerModelInfo(identifier: string, signal?: AbortSignal): Promise<ContainerModelInfo | null> {
    return (await this.get("/api/container/model_info", { identifier }, signal)) as ContainerModelInfo | null;
  }

  /**
   * モデルの中身(生のバイト列、container-model.md §6.2)と ETag(SHA-256)。
   * juiz は Cache-Control: no-cache と ETag を付けて返すので、ブラウザが If-None-Match で問い合わせ直し、
   * 変わっていなければ 304 で手元の写しを使う
   */
  async containerModel(identifier: string, signal?: AbortSignal): Promise<{ data: ArrayBuffer; sha256: string | null }> {
    const path = "/api/container/model";
    let res: Response;
    try {
      res = await fetch(`${this.baseUrl}${path}?${new URLSearchParams({ identifier })}`, { signal });
    } catch (e) {
      if (e instanceof DOMException && e.name === "AbortError") throw e;
      throw new JuizApiError(`接続できません: ${(e as Error).message}`);
    }
    if (!res.ok) throw new JuizApiError(`${path} が ${res.status} を返しました`, res.status);
    const etag = res.headers.get("ETag");
    const sha256 = etag ? etag.replace(/^W\//, "").replace(/"/g, "") : null;
    return { data: await res.arrayBuffer(), sha256 };
  }

  /** 基準 reference から見た Container の位置姿勢 */
  async containerPose(identifier: string, reference: string, signal?: AbortSignal): Promise<PoseStamped> {
    return (await this.get("/api/container/pose", { identifier, reference }, signal)) as PoseStamped;
  }
}
