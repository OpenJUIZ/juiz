// profile_full の中身は OpenAPI で schema 化されていないので、画面側で型を定義する。
// juiz v0.0.18 の応答を見て必要な項目だけ書いている。足りない項目は使うときに足す。

/** 説明。{ text } の形と文字列の形がある */
export type Description = { text?: string } | string;

/** profile に共通する項目 */
export interface ObjectProfile {
  identifier?: string;
  type_name: string;
  /** ネームスペースを含まない短い名前 */
  name: string;
  /** ネームスペース(`/` 区切り、空ならルート)。ネームスペース対応前の juiz では無い */
  namespace?: string;
  class_name?: string;
  broker_type_name?: string;
  broker_name?: string;
  description?: Description;
}

/** 引数 */
export interface ArgumentProfile {
  name: string;
  type_name: string;
  default?: unknown;
  description?: Description;
}

/** 話題の参照(処理の publishes / subscribes) */
export interface TopicRef {
  /** ネームスペースを含めた名前(robot1/value_topic) */
  name: string;
}

/** 処理の識別に使う項目(接続の両端、実行文脈の対象) */
export interface ProcessRef {
  class_name?: string;
  name: string;
  namespace?: string;
  type_name: string;
  container_name?: string | null;
  broker_name?: string;
  broker_type_name?: string;
}

export interface ProcessProfile extends ObjectProfile {
  arguments?: ArgumentProfile[];
  factory?: string;
  language?: string;
  use_memo?: boolean;
  publishes?: TopicRef[];
  /** 引数名 → 話題 */
  subscribes?: Record<string, TopicRef>;
  container_name?: string | null;
  container_type?: string | null;
  /** 戻値の型。今は pyjuiz だけが埋める */
  outputs?: unknown;
}

export type ContainerProcessProfile = ProcessProfile;

export interface ContainerProfile extends ObjectProfile {
  arguments?: ArgumentProfile[];
  /** 生成時に渡した引数 */
  args?: Record<string, unknown>;
  factory?: string;
  language?: string;
  parent_name?: string | null;
  parent_type_name?: string | null;
}

export interface ExecutionContextProfile extends ObjectProfile {
  state?: string;
  rate?: number;
  auto_start?: boolean;
  targets?: ProcessRef[];
}

export type BrokerProfile = ObjectProfile;

/** 話題。中身は type_name が "topic" の処理の profile */
export type TopicProfile = ProcessProfile;

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

/**
 * GET /api/system/profile_full
 *
 * recursive=true を付けると、subsystems に副系の profile が同じ形で再帰的に入り、
 * 各系が持つ接続が connections に入る(#116)。
 * 付けない(または古い juiz)と、subsystems には副系へ経由する仲介の profile だけが入る。
 */
export interface SystemProfile extends ObjectProfile {
  /** 副系へ経由する仲介の profile だけのときは無い */
  core_store?: CoreStore;
  system_store?: { uuid?: string } & Record<string, unknown>;
  subsystems?: SystemProfile[];
  mastersystem?: ObjectProfile | null;
  /** この系が持つ接続。両端が別の系の接続は、両方の系に入る(recursive=true のときだけ) */
  connections?: ConnectionProfile[];
  /** 親の系から副系へ経由する仲介の profile(recursive=true のときだけ) */
  broker_proxy?: ObjectProfile;
  /** 副系の中身を取れなかったときの理由 */
  error?: string;
}

/** GET /api/connection/profile_full */
export interface ConnectionProfile {
  identifier?: string;
  arg_name: string;
  connection_type: string;
  source_process_id: ProcessRef;
  destination_process_id: ProcessRef;
}
