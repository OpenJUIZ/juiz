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
  /** エラーの応答の本文(読めたとき) */
  readonly body: string | undefined;

  constructor(message: string, status?: number, body?: string) {
    super(message);
    this.name = "JuizApiError";
    this.status = status;
    this.body = body;
  }
}

/** profile を読める API の種類 */
type ProfileClass = "system" | "connection" | "transform";

/** `<class>/profile` を持たない古い juiz と分かった接続先(baseUrl)。以後は `<class>/profile_full` を呼ぶ */
const legacyProfileServers = new Set<string>();

/**
 * 相手の juiz にその API が無いというエラーか。新しい juiz は 404 を、
 * 古い juiz は 500 と「CRUDBroker can not detect function」の文を返す(manifest-profile.md §7 の 4)
 */
function isFunctionNotFound(e: unknown): boolean {
  if (!(e instanceof JuizApiError)) return false;
  if (e.status === 404) return true;
  return e.status === 500 && (e.body ?? "").includes("CRUDBroker can not detect function (");
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
      const body = await res.text().catch(() => undefined);
      throw new JuizApiError(`${path} が ${res.status} を返しました`, res.status, body);
    }
    return res.json();
  }

  /**
   * `<className>/profile` を読む。相手が古い juiz で `profile` が無ければ `profile_full` を呼び直す
   * (docs/spec/manifest-profile.md §7 の 4)。古いと分かった相手には、以後はじめから `profile_full` を呼ぶ
   */
  async getProfile(
    className: ProfileClass,
    query: Record<string, string> | undefined,
    signal?: AbortSignal,
  ): Promise<unknown> {
    if (!legacyProfileServers.has(this.baseUrl)) {
      try {
        return await this.get(`/api/${className}/profile`, query as never, signal);
      } catch (e) {
        if (!isFunctionNotFound(e)) throw e;
        legacyProfileServers.add(this.baseUrl);
      }
    }
    return this.get(`/api/${className}/profile_full`, query as never, signal);
  }

  /**
   * 処理・容器処理を呼ぶ(PATCH /api/process/call など、#197)。
   * fn が call_partial なら一部の引数だけで呼び(省いた引数は拘束値か既定値)、p_apply なら引数を拘束する。
   * 戻り値が画像なら Blob、それ以外は JSON の値。エラーなら応答の message を持つ JuizApiError
   */
  async call(
    kind: "process" | "container_process",
    identifier: string,
    args: Record<string, unknown>,
    signal?: AbortSignal,
    fn: "call" | "call_partial" | "p_apply" = "call",
  ): Promise<{ value: unknown } | { image: Blob }> {
    const path = `/api/${kind}/${fn}`;
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
  async systemProfile(recursive: boolean, signal?: AbortSignal): Promise<SystemProfile> {
    const query = recursive ? { recursive: "true" } : undefined;
    return (await this.getProfile("system", query, signal)) as SystemProfile;
  }

  /**
   * 接続を作る(POST /api/connection/create)。source の出力を destination の引数 argName へつなぐ。
   * 型が合わないなどで作れなければ、応答の message を持つ JuizApiError
   */
  async connectionCreate(
    source: string,
    destination: string,
    argName: string,
    connectionType: "push" | "pull",
  ): Promise<ConnectionProfile> {
    return (await this.send("POST", "/api/connection/create", undefined, {
      __manifest__: {
        connection_type: connectionType,
        source_process_id: source,
        destination_process_id: destination,
        arg_name: argName,
      },
    })) as ConnectionProfile;
  }

  /** 接続を外す(DELETE /api/connection/destroy、識別子は connection://送り元|引数|送り先) */
  async connectionDestroy(identifier: string): Promise<ConnectionProfile> {
    return (await this.send("DELETE", "/api/connection/destroy", { identifier })) as ConnectionProfile;
  }

  /** 処理・容器処理・容器を消す(DELETE /api/<kind>/destroy) */
  async destroy(kind: "process" | "container_process" | "container", identifier: string): Promise<unknown> {
    return await this.send("DELETE", `/api/${kind}/destroy`, { identifier });
  }

  /** GET 以外の API を呼ぶ。エラーなら応答の message(無ければ状態コード)を持つ JuizApiError */
  private async send(
    method: "POST" | "DELETE",
    path: string,
    query?: Record<string, string>,
    body?: unknown,
  ): Promise<unknown> {
    const qs = query ? `?${new URLSearchParams(query)}` : "";
    let res: Response;
    try {
      res = await fetch(`${this.baseUrl}${path}${qs}`, {
        method,
        headers: body === undefined ? undefined : { "Content-Type": "application/json" },
        body: body === undefined ? undefined : JSON.stringify(body),
      });
    } catch (e) {
      throw new JuizApiError(`接続できません: ${(e as Error).message}`);
    }
    if (!res.ok) {
      let message = `${path} が ${res.status} を返しました`;
      try {
        const b = (await res.json()) as { message?: string };
        if (b.message) message = b.message;
      } catch {
        // 本文が JSON でなければ状態コードだけ
      }
      throw new JuizApiError(message, res.status);
    }
    return await res.json();
  }

  /** 接続の識別子の一覧 */
  async connectionList(signal?: AbortSignal): Promise<string[]> {
    return (await this.get("/api/connection/list", undefined, signal)) as string[];
  }

  async connectionProfile(identifier: string, signal?: AbortSignal): Promise<ConnectionProfile> {
    return (await this.getProfile("connection", { identifier }, signal)) as ConnectionProfile;
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

  async transformProfile(identifier: string, signal?: AbortSignal): Promise<TransformProfile> {
    return (await this.getProfile("transform", { identifier }, signal)) as TransformProfile;
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
