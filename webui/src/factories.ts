// 系の生成器(Factory)。左のツリーに並べ、画面から処理・容器・容器処理を作るのに使う。
import type { ArgumentProfile, FactoryProfile } from "./api/types.ts";
import type { SystemModel } from "./model.ts";

export type FactoryKind = "process" | "container" | "containerProcess";

export interface FactoryInfo {
  kind: FactoryKind;
  typeName: string;
  language: string | null;
  /** 容器の生成器なら、作るときに渡す引数(conf の容器の項目と同じ)。処理の引数は呼び出すときに渡す */
  arguments: ArgumentProfile[];
}

function info(kind: FactoryKind, f: FactoryProfile): FactoryInfo {
  const inner = f.given_process_factory ?? f.container_factory ?? f.container_process_factory;
  return {
    kind,
    typeName: f.type_name,
    language: f.language ?? null,
    arguments: inner?.arguments ?? [],
  };
}

const byType = (a: FactoryInfo, b: FactoryInfo) => a.typeName.localeCompare(b.typeName);

/** 系の生成器。中身を取れない系や古い juiz では null */
export function systemFactories(system: SystemModel): Record<FactoryKind, FactoryInfo[]> | null {
  const cs = system.profile.core_store;
  if (!cs || (!cs.process_factories && !cs.container_factories && !cs.container_process_factories)) return null;
  return {
    process: (cs.process_factories ?? []).map((f) => info("process", f)).sort(byType),
    container: (cs.container_factories ?? []).map((f) => info("container", f)).sort(byType),
    containerProcess: (cs.container_process_factories ?? []).map((f) => info("containerProcess", f)).sort(byType),
  };
}

/** 名前の候補。型名の後ろに、まだ使われていない一番小さい番号を付ける(conf の慣わしと同じ) */
export function nextName(typeName: string, taken: Iterable<string>): string {
  const used = new Set(taken);
  for (let i = 0; ; i++) {
    const name = `${typeName}${i}`;
    if (!used.has(name)) return name;
  }
}
