// 画面の用語。漢字を正とし、英語名(API やドキュメントでの名前)を小さく並記する(#111)。
export const TERMS = {
  system: { ja: "系", en: "System" },
  subsystem: { ja: "副系", en: "Subsystem" },
  broker: { ja: "仲介", en: "Broker" },
  process: { ja: "処理", en: "Process" },
  container: { ja: "容器", en: "Container" },
  containerProcess: { ja: "容器処理", en: "ContainerProcess" },
  ec: { ja: "実行文脈", en: "ExecutionContext" },
  connection: { ja: "接続", en: "Connection" },
  topic: { ja: "話題", en: "Topic" },
  factory: { ja: "生成器", en: "Factory" },
  namespace: { ja: "名前空間", en: "Namespace" },
  composition: { ja: "合成", en: "Composition" },
  slot: { ja: "差込口", en: "Slot" },
  transformations: { ja: "変換木", en: "Transformations" },
  anchor: { ja: "基点", en: "Anchor" },
  transform: { ja: "変換", en: "Transform" },
  pose: { ja: "位置姿勢", en: "ContainerPose" },
  reference: { ja: "基準", en: "Reference" },
  replica: { ja: "写し", en: "Replica" },
  remoteChild: { ja: "別系の子", en: "Remote child" },
} as const;

export type TermKey = keyof typeof TERMS;
