// 生成器(Factory)から処理・容器・容器処理を生成するダイアログ。左のツリーの生成器の「＋」から開く。
import { useEffect, useMemo, useState } from "react";
import { JuizClient } from "../api/client.ts";
import { nextName, type FactoryInfo } from "../factories.ts";
import type { Model } from "../model.ts";
import { TERMS } from "../terms.ts";
import { argInitial, argType } from "../args.ts";
import { ValueEditor } from "./CallForm.tsx";

interface Props {
  model: Model;
  factory: FactoryInfo;
  serverUrl: string;
  onDone: (message: string) => void;
  onClose: () => void;
}

const API_KIND = { process: "process", container: "container", containerProcess: "container_process" } as const;

export function CreateDialog({ model, factory, serverUrl, onDone, onClose }: Props) {
  const root = model.root;
  const taken = useMemo(() => {
    const list =
      factory.kind === "process" ? root.processes : factory.kind === "container" ? root.containers : root.containerProcesses;
    return list.map((e) => e.fullName);
  }, [root, factory.kind]);
  const [name, setName] = useState(() => nextName(factory.typeName, taken));
  const [namespace, setNamespace] = useState("");

  // 容器の引数
  const args = useMemo(
    () => (factory.kind === "container" ? factory.arguments.map((a) => ({ arg: a, type: argType(a) })) : []),
    [factory],
  );
  const [values, setValues] = useState<Record<string, unknown>>(() =>
    Object.fromEntries(args.filter(({ type }) => type.kind !== "image").map(({ arg, type }) => [arg.name, argInitial(arg, type)])),
  );

  // 容器処理を入れる容器。同じ型の容器処理がすでに入っている容器の型を優先して選んでおく
  const containerTypeHint = useMemo(
    () => root.containerProcesses.find((cp) => cp.typeName === factory.typeName)?.profile.container_type ?? null,
    [root, factory.typeName],
  );
  const containers = useMemo(() => {
    const list = [...root.containers];
    if (containerTypeHint) list.sort((a, b) => Number(b.typeName === containerTypeHint) - Number(a.typeName === containerTypeHint));
    return list;
  }, [root, containerTypeHint]);
  const [containerKey, setContainerKey] = useState(() => containers[0]?.key ?? "");

  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  const nameTaken = taken.includes(namespace ? `${namespace}/${name}` : name);
  const container = containers.find((c) => c.key === containerKey);
  const canSubmit =
    !busy && name.trim() !== "" && !nameTaken && (factory.kind !== "containerProcess" || !!container);

  const submit = async () => {
    setBusy(true);
    setError(null);
    try {
      await new JuizClient(serverUrl).create(
        API_KIND[factory.kind],
        { type_name: factory.typeName, name: name.trim(), ...(namespace.trim() ? { namespace: namespace.trim() } : {}) },
        { args: factory.kind === "container" ? values : undefined, container: container?.identifier },
      );
      onDone(`${TERMS[factory.kind].ja} "${name.trim()}" を生成しました`);
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="dialog-backdrop" onClick={onClose}>
      <form
        className="dialog create-dialog"
        role="dialog"
        aria-label="生成"
        onClick={(e) => e.stopPropagation()}
        onSubmit={(e) => {
          e.preventDefault();
          if (canSubmit) void submit();
        }}
      >
        <h2>
          {TERMS[factory.kind].ja}を生成
          <span className="create-dialog-type">{factory.typeName}</span>
        </h2>
        <label>
          名前
          <input type="text" value={name} autoFocus onChange={(e) => setName(e.target.value)} />
        </label>
        {nameTaken && <div className="error-text">同じ名前がすでにあります</div>}
        <label>
          {TERMS.namespace.ja} <span className="term-en">{TERMS.namespace.en}</span>
          <input type="text" value={namespace} placeholder="空ならルート" onChange={(e) => setNamespace(e.target.value)} />
        </label>
        {factory.kind === "containerProcess" && (
          <label>
            入れる{TERMS.container.ja}
            {containers.length === 0 ? (
              <div className="error-text">{TERMS.container.ja}がありません。先に{TERMS.container.ja}を生成してください</div>
            ) : (
              <select value={containerKey} onChange={(e) => setContainerKey(e.target.value)}>
                {containers.map((c) => (
                  <option key={c.key} value={c.key}>
                    {c.fullName} ({c.typeName})
                  </option>
                ))}
              </select>
            )}
          </label>
        )}
        {args.length > 0 && (
          <div className="call-form">
            <div className="dim">引数</div>
            <div className="call-args">
              {args.map(({ arg: a, type }) => (
                <div key={a.name} className="call-arg">
                  <div className="call-arg-head">
                    <code className="mono">{a.name}</code> <span className="dim">{a.type_name}</span>
                  </div>
                  <ValueEditor
                    type={type}
                    value={values[a.name]}
                    onChange={(v) => setValues((prev) => ({ ...prev, [a.name]: v }))}
                  />
                </div>
              ))}
            </div>
          </div>
        )}
        {factory.kind === "process" && <p className="hint">引数は呼び出すときに渡します(図で選んで「呼びだし」「部分拘束」)。</p>}
        {error && <pre className="error-text">{error}</pre>}
        <div className="buttons">
          <button type="button" onClick={onClose}>
            やめる
          </button>
          <button type="submit" disabled={!canSubmit}>
            {busy ? "生成中…" : "生成する"}
          </button>
        </div>
      </form>
    </div>
  );
}
