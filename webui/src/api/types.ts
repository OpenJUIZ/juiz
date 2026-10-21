// profile_full の中身は OpenAPI で schema 化されていないので、画面側で型を定義する。
// juiz v0.0.18 の応答を見て必要な項目だけ書いている。足りない項目は使うときに足す。

/** profile に共通する項目 */
export interface ObjectProfile {
  identifier?: string;
  type_name: string;
  name: string;
  class_name?: string;
  broker_type_name?: string;
  broker_name?: string;
  description?: { text?: string } | string;
}

export type ProcessProfile = ObjectProfile & Record<string, unknown>;
export type ContainerProfile = ObjectProfile & Record<string, unknown>;
export type ContainerProcessProfile = ObjectProfile & {
  container_name?: string;
  container_type?: string;
} & Record<string, unknown>;
export type ExecutionContextProfile = ObjectProfile & {
  state?: string;
  rate?: number;
} & Record<string, unknown>;
export type BrokerProfile = ObjectProfile & Record<string, unknown>;
export type TopicProfile = Record<string, unknown>;

/** system/profile_full の core_store */
export interface CoreStore {
  processes: ProcessProfile[];
  containers: ContainerProfile[];
  container_processes: ContainerProcessProfile[];
  /** EC は識別子をキーにした辞書で返る */
  ecs: Record<string, ExecutionContextProfile>;
  brokers: BrokerProfile[];
  topics: TopicProfile[];
}

/** GET /api/system/profile_full */
export interface SystemProfile extends ObjectProfile {
  core_store: CoreStore;
  system_store?: { uuid?: string } & Record<string, unknown>;
  /** 別プロセスの juiz(副系)。同じ形の profile が再帰的に入る */
  subsystems: SystemProfile[];
  mastersystem: SystemProfile | null;
}
