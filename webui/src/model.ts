// juiz から取った profile を、ツリーと性質欄で使う形に組み直す。
// 要素どうしの関係(接続の両端、実行文脈の対象、容器と容器処理、話題の発行・購読)はここで解決しておく。

import type {
  BrokerProfile,
  ConnectionProfile,
  ContainerProfile,
  ExecutionContextProfile,
  ProcessProfile,
  ProcessRef,
  SystemProfile,
  TopicProfile,
} from "./api/types.ts";
import type { TermKey } from "./terms.ts";

export type ElementKind =
  | "system"
  | "process"
  | "container"
  | "containerProcess"
  | "ec"
  | "connection"
  | "topic"
  | "broker";

interface ElementBase {
  /** 画面の中で要素を指す鍵。選択状態などに使う */
  key: string;
  /** ネームスペースを含まない短い名前 */
  name: string;
  /** ネームスペース(`/` 区切り、空ならルート)。ネームスペースの無い要素(系、接続、仲介)は空 */
  namespace: string;
  /** ネームスペースを含めた名前(`<namespace>/<name>`、ルートなら name と同じ) */
  fullName: string;
  typeName: string;
  /** juiz の識別子(API に渡す形) */
  identifier: string;
  /** 属する系の鍵 */
  systemKey: string;
}

export interface ProcessElement extends ElementBase {
  kind: "process" | "containerProcess";
  profile: ProcessProfile;
  /** 容器処理のときの所属容器 */
  containerKey: string | null;
  /** この処理の引数へ入る接続 */
  inputs: ConnectionElement[];
  /** この処理から出る接続 */
  outputs: ConnectionElement[];
  /** この処理を対象にする実行文脈 */
  ecs: EcElement[];
}

export interface ContainerElement extends ElementBase {
  kind: "container";
  profile: ContainerProfile;
  processes: ProcessElement[];
  /** CompositeContainer のスロット。key は参照先の容器(系の中に見つからなければ null) */
  slots: { name: string; identifier: string; key: string | null }[];
  /** この容器をスロットで参照している CompositeContainer */
  referencedBy: ContainerElement[];
}

export interface EcElement extends ElementBase {
  kind: "ec";
  profile: ExecutionContextProfile;
  /** 対象の処理。系の中に見つからないものは ref だけ持つ */
  targets: { ref: ProcessRef; key: string | null }[];
}

export interface ConnectionElement extends ElementBase {
  kind: "connection";
  profile: ConnectionProfile;
  sourceKey: string | null;
  destinationKey: string | null;
}

export interface TopicElement extends ElementBase {
  kind: "topic";
  profile: TopicProfile;
  publishers: ProcessElement[];
  /** 購読している処理と、受け取る引数名 */
  subscribers: { process: ProcessElement; argName: string }[];
}

export interface BrokerElement extends ElementBase {
  kind: "broker";
  profile: BrokerProfile;
}

export interface SystemModel extends ElementBase {
  kind: "system";
  profile: SystemProfile;
  uuid: string | null;
  /** 取得に使った URL。副系は分からないので null */
  url: string | null;
  /** 副系のとき、親の系から経由する仲介(http の host:port、qmp など) */
  via: { name: string; typeName: string } | null;
  /** 副系の中身を取れなかったときの理由 */
  error: string | null;
  /** この系を指す仲介の名前("種類|名前")。接続の端がどの系かを探すのに使う */
  addresses: string[];
  processes: ProcessElement[];
  containers: ContainerElement[];
  containerProcesses: ProcessElement[];
  ecs: EcElement[];
  /** この系から出る接続(出す側が分からないものは、返した系に置く)。古い juiz で副系の接続が分からないときは null */
  connections: ConnectionElement[] | null;
  topics: TopicElement[];
  brokers: BrokerElement[];
  subsystems: SystemModel[];
}

export type Element =
  | SystemModel
  | ProcessElement
  | ContainerElement
  | EcElement
  | ConnectionElement
  | TopicElement
  | BrokerElement;

export interface Model {
  root: SystemModel;
  /** 鍵から要素を引く */
  elements: Map<string, Element>;
}

/** 容器処理の type_name は API によって "型:容器名" で返るので型だけにそろえる */
function baseTypeName(typeName: string): string {
  const i = typeName.indexOf(":");
  return i < 0 ? typeName : typeName.slice(0, i);
}

function elementKey(systemKey: string, kind: ElementKind, fullName: string, typeName: string, extra = ""): string {
  return `${systemKey}/${kind}/${extra}${fullName}::${baseTypeName(typeName)}`;
}

/** ネームスペースと名前をつなぐ。ネームスペースが空なら名前だけ(juiz_sdk の join_namespace と同じ) */
export function joinNamespace(namespace: string, name: string): string {
  return namespace ? `${namespace}/${name}` : name;
}

/** "robot1/arm/inc0" を ["robot1/arm", "inc0"] に分ける。`/` が無ければネームスペースは空 */
export function splitNamespace(path: string): [string, string] {
  const i = path.lastIndexOf("/");
  return i < 0 ? ["", path] : [path.slice(0, i), path.slice(i + 1)];
}

/**
 * profile のネームスペース。古い juiz(namespace を返さない)ではルートとみなす。
 * 識別子があれば、そこからも取れる(<種類>://<名前>/<class>/<namespace>/<name>::<type>)
 */
function namespaceOf(p: { namespace?: string; identifier?: string }): string {
  if (p.namespace !== undefined) return p.namespace;
  const m = p.identifier ? /^.+?:\/\/.+?\/.+?\/(.+?)::.+$/.exec(p.identifier) : null;
  return m?.[1] ? splitNamespace(m[1])[0] : "";
}

/** 接続の端や実行文脈の対象を、画面に出す名前にする */
export function refName(ref: ProcessRef): string {
  return joinNamespace(ref.namespace ?? "", ref.name);
}

/** ネームスペースの無い要素の名前の項目 */
function plainName(name: string): Pick<ElementBase, "name" | "namespace" | "fullName"> {
  return { name, namespace: "", fullName: name };
}

/** profile からネームスペース付きの名前の項目を作る */
function nsName(p: { name: string; namespace?: string; identifier?: string }): Pick<ElementBase, "name" | "namespace" | "fullName"> {
  const namespace = namespaceOf(p);
  return { name: p.name, namespace, fullName: joinNamespace(namespace, p.name) };
}

/** ツリーで系の下に並べる分類の鍵 */
export function categoryKey(systemKey: string, term: TermKey): string {
  return `${systemKey}#${term}`;
}

export function descriptionText(d: ProcessProfile["description"]): string {
  if (d === undefined) return "";
  return typeof d === "string" ? d : (d.text ?? "");
}

/** 名前順に並べる。API の返す順は呼ぶたびに変わることがあるので、更新でツリーが並び替わらないように */
function byName<T extends { name: string; fullName: string; typeName: string }>(list: T[]): T[] {
  return list.sort((a, b) => a.fullName.localeCompare(b.fullName) || a.typeName.localeCompare(b.typeName));
}

/** 仲介の名前(host:port)の、同じ機械を指す書き方をそろえる */
function normalizeBrokerName(name: string): string {
  const m = /^(localhost|127\.0\.0\.1|0\.0\.0\.0|\[::1\]|::1)(:\d+)?$/.exec(name);
  return m ? `127.0.0.1${m[2] ?? ""}` : name;
}

function addressOf(typeName: string, name: string): string {
  return `${typeName}|${normalizeBrokerName(name)}`;
}

/** 副系の profile から、親の系から経由する仲介を取り出す */
function viaOf(profile: SystemProfile): { name: string; typeName: string } | null {
  // recursive=true なら broker_proxy に入る。古い juiz では profile そのものが仲介の profile
  const p = profile.broker_proxy ?? (profile.core_store ? null : profile);
  return p ? { name: p.name, typeName: p.type_name } : null;
}

/** 系の見出し。最上位は接続先の host:port、副系は経由する仲介の名前 */
function systemLabel(profile: SystemProfile, url: string | null, via: SystemModel["via"]): string {
  if (url) {
    try {
      return new URL(url).host;
    } catch {
      return url;
    }
  }
  return via?.name ?? profile.name;
}

/** 系ごとに、接続の端を解決するのに使うもの */
interface SystemIndex {
  system: SystemModel;
  findProcess: (ref: ProcessRef) => ProcessElement | undefined;
  /** この系が持つ接続(profile の connections、または最上位で別に取ったもの) */
  connections: ConnectionProfile[] | null;
}

/** 話題の発行・購読の接続か。発行・購読は処理の publishes / subscribes で分かるので接続としては扱わない */
function isTopicConnection(c: ConnectionProfile): boolean {
  const isTopic = (r: ProcessRef) => r.class_name === "topic" || r.type_name === "topic";
  return (
    isTopic(c.source_process_id) ||
    isTopic(c.destination_process_id) ||
    (c.identifier ?? "").split("|").some((end) => end.endsWith("::topic"))
  );
}

function buildSystem(
  profile: SystemProfile,
  systemKey: string,
  url: string | null,
  connections: ConnectionProfile[] | null,
  elements: Map<string, Element>,
  index: SystemIndex[],
): SystemModel {
  const store = profile.core_store;
  const via = url ? null : viaOf(profile);
  // 副系の profile の識別子は副系から見た形(core://core/...)なので、経由する仲介の形に直す。
  // */list?recursive=true が返す識別子(http://127.0.0.1:8001/process/... など)と同じになる
  const gid = (id: string): string =>
    via && id.startsWith("core://core/") ? `${via.typeName}://${via.name}/${id.slice("core://core/".length)}` : id;
  const reg = <T extends Element>(e: T): T => {
    elements.set(e.key, e);
    return e;
  };

  const processes = (store?.processes ?? []).map((p) => {
    const n = nsName(p);
    return reg<ProcessElement>({
      kind: "process",
      key: elementKey(systemKey, "process", n.fullName, p.type_name),
      ...n,
      typeName: p.type_name,
      identifier: gid(p.identifier ?? `core://core/process/${n.fullName}::${p.type_name}`),
      systemKey,
      profile: p,
      containerKey: null,
      inputs: [],
      outputs: [],
      ecs: [],
    });
  });

  const containers = (store?.containers ?? []).map((c) => {
    const n = nsName(c);
    return reg<ContainerElement>({
      kind: "container",
      key: elementKey(systemKey, "container", n.fullName, c.type_name),
      ...n,
      typeName: c.type_name,
      identifier: gid(c.identifier ?? `core://core/container/${n.fullName}::${c.type_name}`),
      systemKey,
      profile: c,
      processes: [],
      slots: [],
      referencedBy: [],
    });
  });

  // CompositeContainer のスロットの参照先。識別子は同じ系から見た形なので gid でそろえる(#170)
  for (const cc of containers) {
    for (const [name, id] of Object.entries(cc.profile.slots ?? {})) {
      const target = containers.find((c) => c.identifier === gid(id));
      cc.slots.push({ name, identifier: id, key: target?.key ?? null });
      target?.referencedBy.push(cc);
    }
  }

  const containerProcesses = (store?.container_processes ?? []).map((p) => {
    const typeName = baseTypeName(p.type_name);
    const n = nsName(p);
    // 容器処理は所属容器と同じネームスペースに置かれる
    const container = containers.find(
      (c) =>
        c.name === p.container_name &&
        c.namespace === n.namespace &&
        (!p.container_type || c.typeName === p.container_type),
    );
    const e = reg<ProcessElement>({
      kind: "containerProcess",
      key: elementKey(systemKey, "containerProcess", n.fullName, typeName, `${p.container_name ?? ""}/`),
      ...n,
      typeName,
      identifier: gid(
        p.identifier ?? `core://core/container_process/${n.fullName}::${typeName}:${p.container_name ?? ""}`,
      ),
      systemKey,
      profile: p,
      containerKey: container?.key ?? null,
      inputs: [],
      outputs: [],
      ecs: [],
    });
    container?.processes.push(e);
    return e;
  });

  const findProcess = (ref: ProcessRef): ProcessElement | undefined => {
    const typeName = baseTypeName(ref.type_name);
    const isCp = ref.class_name === "container_process" || !!ref.container_name;
    const list = isCp ? containerProcesses : processes;
    return list.find(
      (p) =>
        p.name === ref.name &&
        p.namespace === (ref.namespace ?? "") &&
        p.typeName === typeName &&
        (!isCp || !ref.container_name || p.profile.container_name === ref.container_name),
    );
  };

  const ecs = Object.entries(store?.ecs ?? {}).map(([id, ec]) => {
    const n = nsName({ identifier: id, ...ec });
    const e = reg<EcElement>({
      kind: "ec",
      key: elementKey(systemKey, "ec", n.fullName, ec.type_name),
      ...n,
      typeName: ec.type_name,
      identifier: gid(ec.identifier ?? id),
      systemKey,
      profile: ec,
      targets: [],
    });
    for (const ref of ec.targets ?? []) {
      const p = findProcess(ref);
      e.targets.push({ ref, key: p?.key ?? null });
      p?.ecs.push(e);
    }
    return e;
  });

  const allProcesses = [...processes, ...containerProcesses];
  // 話題の profile は短い名前と namespace、処理の publishes / subscribes はネームスペースを含めた名前で返る
  const topics = (store?.topics ?? []).map((t) => {
    const n = nsName(t);
    return reg<TopicElement>({
      kind: "topic",
      key: elementKey(systemKey, "topic", n.fullName, t.type_name ?? "topic"),
      ...n,
      typeName: t.type_name ?? "topic",
      identifier: gid(t.identifier ?? n.fullName),
      systemKey,
      profile: t,
      publishers: allProcesses.filter((p) => (p.profile.publishes ?? []).some((r) => r.name === n.fullName)),
      subscribers: allProcesses.flatMap((p) =>
        Object.entries(p.profile.subscribes ?? {})
          .filter(([, r]) => r.name === n.fullName)
          .map(([argName]) => ({ process: p, argName })),
      ),
    });
  });

  const brokers = (store?.brokers ?? []).map((b) =>
    reg<BrokerElement>({
      kind: "broker",
      key: elementKey(systemKey, "broker", b.name, b.type_name),
      ...plainName(b.name),
      typeName: b.type_name,
      identifier: gid(b.identifier ?? `core://core/broker/${b.name}::${b.type_name}`),
      systemKey,
      profile: b,
    }),
  );

  for (const c of containers) byName(c.processes);

  const addresses = brokers.map((b) => addressOf(b.typeName, b.name));
  if (via) addresses.push(addressOf(via.typeName, via.name));
  if (url) {
    try {
      addresses.push(addressOf("http", new URL(url).host));
    } catch {
      // 接続先が URL の形でなければ、仲介の一覧だけで突き合わせる
    }
  }

  const system = reg<SystemModel>({
    kind: "system",
    key: systemKey,
    ...plainName(systemLabel(profile, url, via)),
    typeName: profile.type_name,
    identifier: profile.identifier ?? "",
    systemKey,
    profile,
    uuid: profile.system_store?.uuid ?? null,
    url,
    via,
    error: profile.error ?? (store ? null : "副系の中身を取得できませんでした"),
    addresses,
    processes: byName(processes),
    containers: byName(containers),
    containerProcesses: byName(containerProcesses),
    ecs: byName(ecs),
    connections: connections && [],
    topics: byName(topics),
    brokers: byName(brokers),
    subsystems: [],
  });
  index.push({ system, findProcess, connections });

  system.subsystems = (profile.subsystems ?? []).map((s, i) =>
    buildSystem(s, `${systemKey}.${s.system_store?.uuid ?? i}`, null, s.connections ?? null, elements, index),
  );
  return system;
}

/**
 * 接続をつなぐ。接続の端は、その接続を返した系から見た仲介(core なら自分、http なら host:port)で書かれているので、
 * 仲介の名前から相手の系を探す。両端が別の系の接続は両方の系から返るので、1 本にまとめて出す側の系に置く。
 */
function linkConnections(index: SystemIndex[], elements: Map<string, Element>): void {
  const findSystem = (from: SystemIndex, ref: ProcessRef): SystemIndex | undefined => {
    if (!ref.broker_type_name || ref.broker_type_name === "core" || ref.broker_name === "core") return from;
    const addr = addressOf(ref.broker_type_name, ref.broker_name ?? "");
    return index.find((s) => s.system.addresses.includes(addr));
  };

  for (const from of index) {
    for (const c of from.connections ?? []) {
      if (isTopicConnection(c)) continue;
      const srcSys = findSystem(from, c.source_process_id);
      const dstSys = findSystem(from, c.destination_process_id);
      const src = srcSys?.findProcess(c.source_process_id);
      const dst = dstSys?.findProcess(c.destination_process_id);
      // 出す側の系に置く。出す側が分からなければ、返した系に置く
      const owner = src ? (srcSys ?? from) : from;
      if (!owner.system.connections) continue;
      const identifier =
        src && dst
          ? `${src.key}|${c.arg_name}|${dst.key}`
          : (c.identifier ??
            `connection://${src?.identifier ?? refName(c.source_process_id)}|${c.arg_name}|${dst?.identifier ?? refName(c.destination_process_id)}`);
      const key = `${owner.system.key}/connection/${identifier}`;
      if (elements.has(key)) continue;
      const e: ConnectionElement = {
        kind: "connection",
        key,
        ...plainName(`${refName(c.source_process_id)} → ${refName(c.destination_process_id)}.${c.arg_name}`),
        typeName: c.connection_type,
        identifier: c.identifier ?? identifier,
        systemKey: owner.system.key,
        profile: c,
        sourceKey: src?.key ?? null,
        destinationKey: dst?.key ?? null,
      };
      elements.set(key, e);
      owner.system.connections.push(e);
      src?.outputs.push(e);
      dst?.inputs.push(e);
    }
  }

  for (const { system } of index) {
    if (system.connections) byName(system.connections);
    for (const p of [...system.processes, ...system.containerProcesses]) {
      byName(p.inputs);
      byName(p.outputs);
      byName(p.ecs);
    }
  }
}

/**
 * system/profile_full と接続の profile から画面用の形を作る。
 * url は取得に使った接続先(画面と同じ場所なら location.origin)。
 * connections を渡さなければ、profile の connections(recursive=true で取ったもの)を使う。
 */
export function buildModel(profile: SystemProfile, url: string, connections?: ConnectionProfile[]): Model {
  const elements = new Map<string, Element>();
  const index: SystemIndex[] = [];
  const root = buildSystem(profile, "sys", url, connections ?? profile.connections ?? [], elements, index);
  linkConnections(index, elements);
  return { root, elements };
}
