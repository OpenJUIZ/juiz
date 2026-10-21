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
  name: string;
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
  processes: ProcessElement[];
  containers: ContainerElement[];
  containerProcesses: ProcessElement[];
  ecs: EcElement[];
  /** 接続。副系の接続は取得していないので null */
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

function elementKey(systemKey: string, kind: ElementKind, name: string, typeName: string, extra = ""): string {
  return `${systemKey}/${kind}/${extra}${name}::${baseTypeName(typeName)}`;
}

/** ツリーで系の下に並べる分類の鍵 */
export function categoryKey(systemKey: string, term: TermKey): string {
  return `${systemKey}#${term}`;
}

export function descriptionText(d: ProcessProfile["description"]): string {
  if (d === undefined) return "";
  return typeof d === "string" ? d : (d.text ?? "");
}

function buildSystem(
  profile: SystemProfile,
  systemKey: string,
  url: string | null,
  connections: ConnectionProfile[] | null,
  elements: Map<string, Element>,
): SystemModel {
  const store = profile.core_store;
  const reg = <T extends Element>(e: T): T => {
    elements.set(e.key, e);
    return e;
  };

  const processes = (store.processes ?? []).map((p) =>
    reg<ProcessElement>({
      kind: "process",
      key: elementKey(systemKey, "process", p.name, p.type_name),
      name: p.name,
      typeName: p.type_name,
      identifier: p.identifier ?? `core://core/process/${p.name}::${p.type_name}`,
      systemKey,
      profile: p,
      containerKey: null,
      inputs: [],
      outputs: [],
      ecs: [],
    }),
  );

  const containers = (store.containers ?? []).map((c) =>
    reg<ContainerElement>({
      kind: "container",
      key: elementKey(systemKey, "container", c.name, c.type_name),
      name: c.name,
      typeName: c.type_name,
      identifier: c.identifier ?? `core://core/container/${c.name}::${c.type_name}`,
      systemKey,
      profile: c,
      processes: [],
    }),
  );

  const containerProcesses = (store.container_processes ?? []).map((p) => {
    const typeName = baseTypeName(p.type_name);
    const container = containers.find(
      (c) => c.name === p.container_name && (!p.container_type || c.typeName === p.container_type),
    );
    const e = reg<ProcessElement>({
      kind: "containerProcess",
      key: elementKey(systemKey, "containerProcess", p.name, typeName, `${p.container_name ?? ""}/`),
      name: p.name,
      typeName,
      identifier:
        p.identifier ?? `core://core/container_process/${p.name}::${typeName}:${p.container_name ?? ""}`,
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
        p.typeName === typeName &&
        (!isCp || !ref.container_name || p.profile.container_name === ref.container_name),
    );
  };

  const ecs = Object.entries(store.ecs ?? {}).map(([id, ec]) => {
    const e = reg<EcElement>({
      kind: "ec",
      key: elementKey(systemKey, "ec", ec.name, ec.type_name),
      name: ec.name,
      typeName: ec.type_name,
      identifier: ec.identifier ?? id,
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

  const conns =
    connections?.map((c) => {
      const src = findProcess(c.source_process_id);
      const dst = findProcess(c.destination_process_id);
      const identifier =
        c.identifier ??
        `connection://${src?.identifier ?? c.source_process_id.name}|${c.arg_name}|${dst?.identifier ?? c.destination_process_id.name}`;
      const e = reg<ConnectionElement>({
        kind: "connection",
        key: `${systemKey}/connection/${identifier}`,
        name: `${c.source_process_id.name} → ${c.destination_process_id.name}.${c.arg_name}`,
        typeName: c.connection_type,
        identifier,
        systemKey,
        profile: c,
        sourceKey: src?.key ?? null,
        destinationKey: dst?.key ?? null,
      });
      src?.outputs.push(e);
      dst?.inputs.push(e);
      return e;
    }) ?? null;

  const allProcesses = [...processes, ...containerProcesses];
  const topics = (store.topics ?? []).map((t) =>
    reg<TopicElement>({
      kind: "topic",
      key: elementKey(systemKey, "topic", t.name, t.type_name ?? "topic"),
      name: t.name,
      typeName: t.type_name ?? "topic",
      identifier: t.identifier ?? t.name,
      systemKey,
      profile: t,
      publishers: allProcesses.filter((p) => (p.profile.publishes ?? []).some((r) => r.name === t.name)),
      subscribers: allProcesses.flatMap((p) =>
        Object.entries(p.profile.subscribes ?? {})
          .filter(([, r]) => r.name === t.name)
          .map(([argName]) => ({ process: p, argName })),
      ),
    }),
  );

  const brokers = (store.brokers ?? []).map((b) =>
    reg<BrokerElement>({
      kind: "broker",
      key: elementKey(systemKey, "broker", b.name, b.type_name),
      name: b.name,
      typeName: b.type_name,
      identifier: b.identifier ?? `core://core/broker/${b.name}::${b.type_name}`,
      systemKey,
      profile: b,
    }),
  );

  const subsystems = (profile.subsystems ?? []).map((s, i) =>
    buildSystem(s, `${systemKey}.${s.system_store?.uuid ?? i}`, null, null, elements),
  );

  return reg<SystemModel>({
    kind: "system",
    key: systemKey,
    name: profile.name,
    typeName: profile.type_name,
    identifier: profile.identifier ?? "",
    systemKey,
    profile,
    uuid: profile.system_store?.uuid ?? null,
    url,
    processes,
    containers,
    containerProcesses,
    ecs,
    connections: conns,
    topics,
    brokers,
    subsystems,
  });
}

/**
 * system/profile_full と接続の profile から画面用の形を作る。
 * url は取得に使った接続先(画面と同じ場所なら location.origin)。
 */
export function buildModel(profile: SystemProfile, connections: ConnectionProfile[], url: string): Model {
  const elements = new Map<string, Element>();
  const root = buildSystem(profile, "sys", url, connections, elements);
  return { root, elements };
}
