// 図の上で処理の出力を別の処理へドラッグしたときに出す、つなぐ引数を選ぶメニュー。
import { useEffect, useRef, useState } from "react";
import type { ProcessElement } from "../model.ts";

export interface ConnectRequest {
  source: ProcessElement;
  destination: ProcessElement;
  /** メニューを出す位置(図の枠からの座標) */
  x: number;
  y: number;
}

interface Props {
  request: ConnectRequest;
  busy: boolean;
  onConnect: (argName: string, connectionType: "push" | "pull") => void;
  onClose: () => void;
}

/** 戻り値の型名。型付きでなければ null */
function outputTypeName(p: ProcessElement): string | null {
  return p.profile.output_schema?.name ?? null;
}

export function ConnectMenu({ request, busy, onConnect, onClose }: Props) {
  const { source, destination, x, y } = request;
  const [connectionType, setConnectionType] = useState<"push" | "pull">("push");
  const ref = useRef<HTMLDivElement>(null);
  const args = destination.profile.arguments ?? [];
  const connectedArgs = new Set(destination.inputs.map((c) => c.profile.arg_name));
  const subscribed = new Set(Object.keys(destination.profile.subscribes ?? {}));
  const outType = outputTypeName(source);

  // Esc とメニューの外のクリックで閉じる
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    const onDown = (e: PointerEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) onClose();
    };
    window.addEventListener("keydown", onKey);
    window.addEventListener("pointerdown", onDown, true);
    return () => {
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("pointerdown", onDown, true);
    };
  }, [onClose]);

  // 引数が 1 つなら最初の項目に、そうでなければメニューに焦点を移す(キーボードで選べるように)
  useEffect(() => {
    ref.current?.querySelector<HTMLButtonElement>("button.connect-arg")?.focus();
  }, []);

  return (
    <div ref={ref} className="connect-menu" style={{ left: x, top: y }} role="dialog" aria-label="つなぐ引数を選ぶ">
      <div className="connect-menu-head">
        <span className="connect-menu-from">{source.fullName}</span>
        {outType && <span className="connect-menu-type">{outType}</span>}
        <span className="connect-menu-arrow">→</span>
        <span className="connect-menu-to">{destination.fullName}</span>
      </div>
      <div className="connect-menu-kind" role="radiogroup" aria-label="接続の種類">
        {(
          [
            ["push", "押込", "送り元が実行されたら、その出力を送り先へ送って実行する"],
            ["pull", "引込", "送り先が実行されるときに、送り元を実行してその出力を使う"],
          ] as const
        ).map(([value, label, title]) => (
          <button
            key={value}
            type="button"
            role="radio"
            aria-checked={connectionType === value}
            title={title}
            onClick={() => setConnectionType(value)}
          >
            {label}
          </button>
        ))}
      </div>
      {args.length === 0 ? (
        <div className="connect-menu-empty">この処理には引数がありません</div>
      ) : (
        <ul>
          {args.map((a) => {
            const state = connectedArgs.has(a.name)
              ? "接続あり"
              : subscribed.has(a.name)
                ? "購読中"
                : a.value !== undefined
                  ? "拘束中"
                  : null;
            return (
              <li key={a.name}>
                <button
                  type="button"
                  className="connect-arg"
                  disabled={busy}
                  onClick={() => onConnect(a.name, connectionType)}
                >
                  <span className="connect-arg-name">{a.name}</span>
                  <span className="connect-arg-type">{a.type_name}</span>
                  {state && <span className="connect-arg-state">{state}</span>}
                  {/* 互換の判定は juiz が行う(datatype.md §8)。ここでは名前が違うことだけ知らせる */}
                  {outType && a.schema && a.schema.name !== outType && (
                    <span className="connect-arg-warn" title={`出力は ${outType}`}>
                      型が合わない?
                    </span>
                  )}
                </button>
              </li>
            );
          })}
        </ul>
      )}
    </div>
  );
}
