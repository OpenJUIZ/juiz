import { useEffect, useMemo, useState } from "react";
import type { ArgumentProfile } from "../api/types.ts";
import { JuizClient } from "../api/client.ts";
import { initialValue, isObject, parseAvro, typeLabel, type AvroType } from "../avro.ts";
import { descriptionText, type ProcessElement } from "../model.ts";

// 処理を呼ぶフォーム(#197)。型付きの引数は型参照の Avro のスキーマから、
// 型なしの引数は type_name から入力欄を作る。値は辞書の表し方(仕様 datatype.md §4)で送る。

interface Props {
  element: ProcessElement;
  serverUrl: string;
  /**
   * call: 呼び出す。渡す引数を外すと call_partial で呼ぶ(外した引数は拘束値か既定値)。
   * p_apply: 渡す引数を拘束する(部分拘束)。入力欄の初めの値は今の拘束値
   */
  mode?: "call" | "p_apply";
  /** 呼び出せた・拘束できたとき */
  onDone?: () => void;
}

/** 型なしの引数の type_name を型にする。Array・Object は JSON で入力する */
function untypedArgType(typeName: string): AvroType {
  switch (typeName) {
    case "Bool":
      return { kind: "boolean" };
    case "Int":
      return { kind: "long" };
    case "Float":
      return { kind: "double" };
    case "String":
      return { kind: "string" };
    case "Image":
      return { kind: "image" };
    default:
      return { kind: "any" };
  }
}

function argType(a: ArgumentProfile): AvroType {
  return a.schema ? parseAvro(a.schema.avro) : untypedArgType(a.type_name);
}

/** 引数の初期値。manifest の既定値(辞書の表し方)があればそれ */
function argInitial(a: ArgumentProfile, t: AvroType): unknown {
  return a.default !== undefined && a.default !== null ? a.default : initialValue(t);
}

type Result = { value: unknown } | { image: string } | { error: string };

export function CallForm({ element, serverUrl, mode = "call", onDone }: Props) {
  const args = useMemo(
    () => (element.profile.arguments ?? []).map((a) => ({ arg: a, type: argType(a) })),
    [element.profile.arguments],
  );
  const initial = useMemo(
    () =>
      Object.fromEntries(
        args.flatMap(({ arg, type }) =>
          type.kind === "image"
            ? []
            : [[arg.name, mode === "p_apply" && arg.value !== undefined ? arg.value : argInitial(arg, type)] as const],
        ),
      ),
    [args, mode],
  );
  const [values, setValues] = useState<Record<string, unknown>>(initial);
  // 渡す引数。外した引数は送らない
  const [omitted, setOmitted] = useState<ReadonlySet<string>>(new Set());
  const [jsonMode, setJsonMode] = useState(false);
  const [jsonText, setJsonText] = useState("");
  const [jsonError, setJsonError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [result, setResult] = useState<Result | null>(null);

  // 画像の結果の URL は使い終わったら捨てる
  useEffect(() => {
    return () => {
      if (result && "image" in result) URL.revokeObjectURL(result.image);
    };
  }, [result]);

  const hasImageArg = args.some(({ type }) => type.kind === "image");

  const toggleJson = () => {
    if (jsonMode) {
      try {
        const v: unknown = JSON.parse(jsonText);
        if (!isObject(v)) throw new Error("引数は {名前: 値} の形の JSON で書きます");
        setValues(v);
        setJsonError(null);
        setJsonMode(false);
      } catch (e) {
        setJsonError((e as Error).message);
      }
    } else {
      setJsonText(JSON.stringify(values, null, 2));
      setJsonError(null);
      setJsonMode(true);
    }
  };

  const submit = async () => {
    let body = values;
    if (jsonMode) {
      try {
        const v: unknown = JSON.parse(jsonText);
        if (!isObject(v)) throw new Error("引数は {名前: 値} の形の JSON で書きます");
        body = v;
        setJsonError(null);
      } catch (e) {
        setJsonError((e as Error).message);
        return;
      }
    }
    if (!jsonMode && omitted.size > 0) body = Object.fromEntries(Object.entries(body).filter(([k]) => !omitted.has(k)));
    const fn = mode === "p_apply" ? "p_apply" : !jsonMode && omitted.size > 0 ? "call_partial" : "call";
    setBusy(true);
    try {
      const kind = element.kind === "containerProcess" ? "container_process" : "process";
      const client = new JuizClient(serverUrl);
      if (fn === "p_apply") {
        // p_apply は 1 回に 1 つの引数を拘束する({arg_name, value})
        for (const [argName, value] of Object.entries(body)) {
          await client.call(kind, element.identifier, { arg_name: argName, value }, undefined, "p_apply");
        }
        setResult({ value: body });
      } else {
        const r = await client.call(kind, element.identifier, body, undefined, fn);
        setResult("image" in r ? { image: URL.createObjectURL(r.image) } : r);
      }
      onDone?.();
    } catch (e) {
      setResult({ error: (e as Error).message });
    } finally {
      setBusy(false);
    }
  };

  return (
    <form
      className="call-form"
      onSubmit={(e) => {
        e.preventDefault();
        void submit();
      }}
    >
      {jsonMode ? (
        <textarea
          className="json-input"
          value={jsonText}
          rows={Math.min(16, jsonText.split("\n").length + 1)}
          spellCheck={false}
          onChange={(e) => setJsonText(e.target.value)}
        />
      ) : args.length === 0 ? (
        <div className="dim">引数はありません</div>
      ) : (
        <div className="call-args">
          {args.map(({ arg: a, type }) =>
            type.kind === "image" ? (
              <div key={a.name} className="call-arg">
                <code className="mono">{a.name}</code> <span className="none">画像の引数はこの画面からは渡せません</span>
              </div>
            ) : (
              <div key={a.name} className="call-arg" title={descriptionText(a.description)}>
                <div className="call-arg-head">
                  <label className="call-arg-include" title={mode === "p_apply" ? "この引数を拘束する" : "この引数を渡す"}>
                    <input
                      type="checkbox"
                      checked={!omitted.has(a.name)}
                      onChange={(e) =>
                        setOmitted((prev) => {
                          const next = new Set(prev);
                          if (e.target.checked) next.delete(a.name);
                          else next.add(a.name);
                          return next;
                        })
                      }
                    />{" "}
                    <code className="mono">{a.name}</code>
                  </label>{" "}
                  <span className="dim">{a.schema ? typeLabel(type) : a.type_name}</span>
                  {mode === "call" && omitted.has(a.name) && (
                    <span className="dim"> {a.value !== undefined ? "拘束値を使う" : "既定値を使う"}</span>
                  )}
                </div>
                {!omitted.has(a.name) && (
                  <ValueEditor
                    type={type}
                    value={values[a.name]}
                    onChange={(v) => setValues((prev) => ({ ...prev, [a.name]: v }))}
                  />
                )}
              </div>
            ),
          )}
        </div>
      )}
      {jsonError && <div className="error-text">{jsonError}</div>}
      <div className="call-buttons">
        <button type="button" onClick={toggleJson}>
          {jsonMode ? "フォームで編集" : "JSON で編集"}
        </button>
        <button
          type="button"
          onClick={() => {
            setValues(initial);
            setOmitted(new Set());
            setJsonMode(false);
            setJsonError(null);
          }}
        >
          既定値に戻す
        </button>
        <button type="submit" disabled={busy}>
          {mode === "p_apply" ? (busy ? "拘束中…" : "拘束する") : busy ? "呼出中…" : "呼び出す"}
        </button>
      </div>
      {hasImageArg && !jsonMode && <div className="dim hint">画像の引数を省くと、処理の既定のふるまいになります</div>}
      {result && (
        <div className="call-result">
          <div className="dim">{mode === "p_apply" && !("error" in result) ? "拘束しました" : "結果"}</div>
          {"error" in result ? (
            <pre className="error-text">{result.error}</pre>
          ) : "image" in result ? (
            <img src={result.image} alt="結果の画像" />
          ) : (
            <pre className="mono">{JSON.stringify(result.value, null, 2)}</pre>
          )}
        </div>
      )}
    </form>
  );
}

interface EditorProps {
  type: AvroType;
  value: unknown;
  onChange: (v: unknown) => void;
}

/** 型に合わせた入力欄。record・array・map は入れ子にする */
export function ValueEditor({ type: t, value, onChange }: EditorProps) {
  switch (t.kind) {
    case "boolean":
      return <input type="checkbox" checked={value === true} onChange={(e) => onChange(e.target.checked)} />;
    case "int":
    case "long":
    case "float":
    case "double":
      return <NumberInput integer={t.kind === "int" || t.kind === "long"} value={value} onChange={onChange} />;
    case "string":
    case "bytes":
      return (
        <input
          type="text"
          value={typeof value === "string" ? value : ""}
          placeholder={t.kind === "bytes" ? "base64" : undefined}
          onChange={(e) => onChange(e.target.value)}
        />
      );
    case "enum":
      return (
        <select value={typeof value === "string" ? value : ""} onChange={(e) => onChange(e.target.value)}>
          {t.symbols.map((s) => (
            <option key={s} value={s}>
              {s}
            </option>
          ))}
        </select>
      );
    case "optional":
      return (
        <div className="optional-input">
          <label className="dim">
            <input
              type="checkbox"
              checked={value === null || value === undefined}
              onChange={(e) => onChange(e.target.checked ? null : initialValue(t.inner))}
            />{" "}
            null
          </label>
          {value !== null && value !== undefined && <ValueEditor type={t.inner} value={value} onChange={onChange} />}
        </div>
      );
    case "record": {
      const obj = isObject(value) ? value : {};
      return (
        <div className="record-input">
          {t.fields.map((f) => (
            <div key={f.name} className="record-field" title={f.doc}>
              <span className="mono">{f.name}</span>
              <ValueEditor
                type={f.type}
                value={f.name in obj ? obj[f.name] : f.default !== undefined ? f.default : initialValue(f.type)}
                onChange={(v) => onChange({ ...obj, [f.name]: v })}
              />
            </div>
          ))}
        </div>
      );
    }
    case "array": {
      const items = Array.isArray(value) ? value : [];
      return (
        <div className="list-input">
          {items.map((item, i) => (
            <div key={i} className="list-item">
              <ValueEditor type={t.items} value={item} onChange={(v) => onChange(items.map((x, j) => (j === i ? v : x)))} />
              <button type="button" title="この要素を消す" onClick={() => onChange(items.filter((_, j) => j !== i))}>
                ×
              </button>
            </div>
          ))}
          <button type="button" onClick={() => onChange([...items, initialValue(t.items)])}>
            ＋ 要素
          </button>
        </div>
      );
    }
    case "map":
      return <MapInput valueType={t.values} value={value} onChange={onChange} />;
    case "null":
      return <span className="dim">null</span>;
    case "image":
      return <span className="none">画像はこの画面からは渡せません</span>;
    case "any":
    case "unknown":
      return <JsonInput value={value} onChange={onChange} />;
  }
}

/** 数の入力。入力中の文字列は手元に持ち、数として読めたときだけ値を変える */
function NumberInput({ integer, value, onChange }: { integer: boolean; value: unknown; onChange: (v: unknown) => void }) {
  const [text, setText] = useState(typeof value === "number" ? String(value) : "");
  // 外から値が変わったとき(既定値に戻す、JSON で編集)だけ入力中の文字列を合わせる
  const [seen, setSeen] = useState(value);
  if (value !== seen) {
    setSeen(value);
    if (typeof value === "number" && Number(text) !== value) setText(String(value));
  }
  const n = Number(text);
  const invalid = text.trim() === "" || !Number.isFinite(n) || (integer && !Number.isInteger(n));
  return (
    <input
      type="text"
      inputMode={integer ? "numeric" : "decimal"}
      className={invalid ? "invalid" : undefined}
      value={text}
      onChange={(e) => {
        setText(e.target.value);
        const v = Number(e.target.value);
        if (e.target.value.trim() !== "" && Number.isFinite(v) && (!integer || Number.isInteger(v))) onChange(v);
      }}
    />
  );
}

/** 何でも入る値(any、型なしの Array・Object)は JSON で入力する */
function JsonInput({ value, onChange }: { value: unknown; onChange: (v: unknown) => void }) {
  const [text, setText] = useState(() => JSON.stringify(value ?? null));
  const [invalid, setInvalid] = useState(false);
  // 外から値が変わったとき(既定値に戻すなど)だけ入力中の文字列を合わせる
  const [seen, setSeen] = useState(value);
  if (value !== seen) {
    setSeen(value);
    let same: boolean;
    try {
      same = JSON.stringify(JSON.parse(text)) === JSON.stringify(value ?? null);
    } catch {
      same = false;
    }
    if (!same) {
      setText(JSON.stringify(value ?? null));
      setInvalid(false);
    }
  }
  return (
    <input
      type="text"
      className={invalid ? "invalid mono" : "mono"}
      value={text}
      placeholder="JSON"
      onChange={(e) => {
        setText(e.target.value);
        try {
          onChange(JSON.parse(e.target.value));
          setInvalid(false);
        } catch {
          setInvalid(true);
        }
      }}
    />
  );
}

/** map<T> の入力。キーと値の組を並べる */
function MapInput({ valueType, value, onChange }: { valueType: AvroType; value: unknown; onChange: (v: unknown) => void }) {
  const entries = Object.entries(isObject(value) ? value : {});
  const [newKey, setNewKey] = useState("");
  const rebuild = (es: [string, unknown][]) => onChange(Object.fromEntries(es));
  return (
    <div className="list-input">
      {entries.map(([k, v], i) => (
        <div key={k} className="list-item">
          <span className="mono">{k}</span>
          <ValueEditor type={valueType} value={v} onChange={(nv) => rebuild(entries.map((e, j) => (j === i ? [k, nv] : e)))} />
          <button type="button" title="このキーを消す" onClick={() => rebuild(entries.filter((_, j) => j !== i))}>
            ×
          </button>
        </div>
      ))}
      <div className="list-item">
        <input type="text" value={newKey} placeholder="キー" onChange={(e) => setNewKey(e.target.value)} />
        <button
          type="button"
          disabled={newKey === "" || entries.some(([k]) => k === newKey)}
          onClick={() => {
            rebuild([...entries, [newKey, initialValue(valueType)]]);
            setNewKey("");
          }}
        >
          ＋ キー
        </button>
      </div>
    </div>
  );
}
