// 型参照の Avro のスキーマ(仕様 docs/spec/datatype.md §3・§6)を、入力フォームで使う形に読む(#197)。
//
// 値は辞書の表し方(§4)で扱う。bytes は base64 の文字列、T? は包まずに値か null。
// 型参照の avro はそれだけで完結している(名前付き型は最初に出てきたところで定義され、後は名前で参照される)。

/** 型参照 {name, fingerprint, avro}(manifest の schema / output_schema) */
export interface TypeSchemaRef {
  name: string;
  fingerprint: string;
  avro: unknown;
}

export interface AvroField {
  name: string;
  doc?: string;
  type: AvroType;
  /** 既定値(辞書の表し方)。無ければ undefined */
  default?: unknown;
}

export type AvroType =
  | { kind: "null" | "boolean" | "int" | "long" | "float" | "double" | "string" | "bytes" | "any" | "image" }
  | { kind: "array"; items: AvroType }
  | { kind: "map"; values: AvroType }
  | { kind: "optional"; inner: AvroType }
  | { kind: "record"; name: string; fullName: string; doc?: string; fields: AvroField[] }
  | { kind: "enum"; name: string; fullName: string; doc?: string; symbols: string[]; default?: string }
  /** 読めなかった型。JSON で入力する */
  | { kind: "unknown"; raw: unknown };

const PRIMITIVES = new Set(["null", "boolean", "int", "long", "float", "double", "string", "bytes"]);
/** any と image は予約名の空のレコード(§3.2・§3.3) */
const ANY_FULL_NAME = "juiz.Any";
const IMAGE_FULL_NAME = "juiz.Image";

function fullNameOf(name: string, namespace: string | undefined): string {
  if (name.includes(".") || !namespace) return name;
  return `${namespace}.${name}`;
}

function nsOf(fullName: string): string {
  const i = fullName.lastIndexOf(".");
  return i < 0 ? "" : fullName.slice(0, i);
}

/** Avro のスキーマ(JSON)を読む。読めない部分は unknown にする */
export function parseAvro(schema: unknown): AvroType {
  return parse(schema, "", new Map());
}

function parse(s: unknown, ns: string, named: Map<string, AvroType>): AvroType {
  if (typeof s === "string") {
    if (PRIMITIVES.has(s)) return { kind: s } as AvroType;
    const full = fullNameOf(s, ns);
    return named.get(full) ?? named.get(s) ?? { kind: "unknown", raw: s };
  }
  if (Array.isArray(s)) {
    // union は ["null", T] か [T, "null"] だけ(§3.1)
    if (s.length === 2 && (s[0] === "null" || s[1] === "null")) {
      const other = s[0] === "null" ? s[1] : s[0];
      return { kind: "optional", inner: parse(other, ns, named) };
    }
    return { kind: "unknown", raw: s };
  }
  if (!s || typeof s !== "object") return { kind: "unknown", raw: s };
  const o = s as Record<string, unknown>;
  const type = o.type;
  if (type === "array") return { kind: "array", items: parse(o.items, ns, named) };
  if (type === "map") return { kind: "map", values: parse(o.values, ns, named) };
  if (type === "record" || type === "enum") {
    const name = String(o.name ?? "");
    const fullName = fullNameOf(name, typeof o.namespace === "string" ? o.namespace : ns);
    const shortName = fullName.slice(fullName.lastIndexOf(".") + 1);
    const doc = typeof o.doc === "string" ? o.doc : undefined;
    if (fullName === ANY_FULL_NAME) return remember(named, fullName, { kind: "any" });
    if (fullName === IMAGE_FULL_NAME) return remember(named, fullName, { kind: "image" });
    if (type === "enum") {
      const symbols = Array.isArray(o.symbols) ? o.symbols.map(String) : [];
      const def = typeof o.default === "string" ? o.default : undefined;
      return remember(named, fullName, { kind: "enum", name: shortName, fullName, doc, symbols, default: def });
    }
    const record: AvroType & { kind: "record" } = { kind: "record", name: shortName, fullName, doc, fields: [] };
    remember(named, fullName, record);
    const inner = nsOf(fullName);
    for (const f of Array.isArray(o.fields) ? o.fields : []) {
      const fo = f as Record<string, unknown>;
      const ft = parse(fo.type, inner, named);
      record.fields.push({
        name: String(fo.name ?? ""),
        doc: typeof fo.doc === "string" ? fo.doc : undefined,
        type: ft,
        default: "default" in fo ? avroDefaultToDict(ft, fo.default) : undefined,
      });
    }
    return record;
  }
  if (typeof type === "string" || Array.isArray(type)) {
    // {"type": "long"} のような書き方
    return parse(type, ns, named);
  }
  return { kind: "unknown", raw: s };
}

function remember(named: Map<string, AvroType>, fullName: string, t: AvroType): AvroType {
  named.set(fullName, t);
  return t;
}

/** Avro の既定値の書き方(bytes は 0〜255 の文字の文字列)を辞書の表し方にする */
function avroDefaultToDict(t: AvroType, v: unknown): unknown {
  switch (t.kind) {
    case "bytes":
      if (typeof v !== "string") return v;
      try {
        return btoa(v);
      } catch {
        return v;
      }
    case "optional":
      return v === null ? null : avroDefaultToDict(t.inner, v);
    case "array":
      return Array.isArray(v) ? v.map((e) => avroDefaultToDict(t.items, e)) : v;
    case "map":
      return isObject(v) ? Object.fromEntries(Object.entries(v).map(([k, e]) => [k, avroDefaultToDict(t.values, e)])) : v;
    case "record":
      if (!isObject(v)) return v;
      return Object.fromEntries(
        Object.entries(v).map(([k, e]) => {
          const f = t.fields.find((x) => x.name === k);
          return [k, f ? avroDefaultToDict(f.type, e) : e];
        }),
      );
    default:
      return v;
  }
}

export function isObject(v: unknown): v is Record<string, unknown> {
  return typeof v === "object" && v !== null && !Array.isArray(v);
}

/** 型の初期値。フィールドの既定値があればそれ、なければ型ごとの空の値 */
export function initialValue(t: AvroType): unknown {
  switch (t.kind) {
    case "null":
    case "optional":
    case "any":
    case "unknown":
    case "image":
      return null;
    case "boolean":
      return false;
    case "int":
    case "long":
    case "float":
    case "double":
      return 0;
    case "string":
    case "bytes":
      return "";
    case "array":
      return [];
    case "map":
      return {};
    case "enum":
      return t.default ?? t.symbols[0] ?? "";
    case "record":
      return Object.fromEntries(t.fields.map((f) => [f.name, f.default !== undefined ? f.default : initialValue(f.type)]));
  }
}

/** 画面に出す型の名前(名前付き型は名前空間を省く) */
export function typeLabel(t: AvroType): string {
  switch (t.kind) {
    case "array":
      return `array<${typeLabel(t.items)}>`;
    case "map":
      return `map<${typeLabel(t.values)}>`;
    case "optional":
      return `${typeLabel(t.inner)}?`;
    case "record":
    case "enum":
      return t.name;
    case "unknown":
      return "?";
    default:
      return t.kind;
  }
}

/** 型参照の表示名(`robot.geometry.Pose2D` → `Pose2D`、`array<a.b.P>` → `array<P>`) */
export function shortTypeName(name: string): string {
  return name.replace(/[A-Za-z_][\w]*(?:\.[A-Za-z_][\w]*)+/g, (m) => m.slice(m.lastIndexOf(".") + 1));
}
