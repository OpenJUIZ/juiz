import type { ElementKind } from "./model.ts";

/** 種類ごとのマークの形と色(layout.css の .mark.*) */
export const MARK_CLASS: Record<ElementKind, string> = {
  system: "system",
  process: "process",
  containerProcess: "process",
  container: "container",
  ec: "ec",
  connection: "connection",
  topic: "topic",
  broker: "other",
};
