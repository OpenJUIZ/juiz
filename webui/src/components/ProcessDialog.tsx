// 照準の輪のメニューから開く、呼びだし・部分拘束のダイアログ。中身は性質欄と同じ CallForm。
import { useEffect } from "react";
import type { ProcessElement } from "../model.ts";
import { TERMS } from "../terms.ts";
import { CallForm } from "./CallForm.tsx";

interface Props {
  element: ProcessElement;
  mode: "call" | "p_apply";
  serverUrl: string;
  onDone: () => void;
  onClose: () => void;
}

export function ProcessDialog({ element, mode, serverUrl, onDone, onClose }: Props) {
  // Esc で閉じる
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  return (
    <div className="dialog-backdrop" onClick={onClose}>
      <div
        className="dialog process-dialog"
        role="dialog"
        aria-label={mode === "call" ? "呼びだし" : "部分拘束"}
        onClick={(e) => e.stopPropagation()}
      >
        <h2>
          {mode === "call" ? "呼びだし" : "部分拘束"}
          <span className="process-dialog-target">
            {TERMS[element.kind].ja} "{element.fullName}"
          </span>
        </h2>
        <p className="hint">
          {mode === "call"
            ? "チェックを外した引数は渡さず、拘束値か既定値で呼び出します(call_partial)。"
            : "チェックした引数の値を拘束します(p_apply)。この後の呼び出しで使われます。"}
        </p>
        <CallForm element={element} serverUrl={serverUrl} mode={mode} onDone={onDone} />
        <div className="buttons">
          <button type="button" onClick={onClose}>
            戻る
          </button>
        </div>
      </div>
    </div>
  );
}
