// 中央の表示。「系の図」(#115)と「3D」(#384)を切り替える。
import { lazy, Suspense, useState } from "react";
import type { Model } from "../model.ts";
import { DiagramPane } from "./DiagramPane.tsx";

// three.js は 3D を開いたときだけ読み込む(container-model.md §12)
const ModelView = lazy(() => import("./ModelView.tsx"));

type View = "diagram" | "3d";

const VIEW_KEY = "juiz-webui.centerView";

function loadView(): View {
  try {
    return localStorage.getItem(VIEW_KEY) === "3d" ? "3d" : "diagram";
  } catch {
    return "diagram";
  }
}

interface Props {
  model: Model | null;
  serverUrl: string;
  selectedKey: string | null;
  onSelect: (key: string | null) => void;
}

export function CenterPane(props: Props) {
  const [view, setView] = useState<View>(loadView);
  const choose = (v: View) => {
    setView(v);
    try {
      localStorage.setItem(VIEW_KEY, v);
    } catch {
      // 覚えられなくても切り替えはできる
    }
  };
  return (
    <main className="diagram-pane">
      {view === "diagram" ? (
        <DiagramPane {...props} />
      ) : (
        <Suspense fallback={<div className="view-loading">3D 表示を読み込み中…</div>}>
          <ModelView {...props} />
        </Suspense>
      )}
      <div className="view-tabs" role="tablist">
        <button type="button" role="tab" aria-selected={view === "diagram"} onClick={() => choose("diagram")}>
          系の図
        </button>
        <button type="button" role="tab" aria-selected={view === "3d"} onClick={() => choose("3d")}>
          3D
        </button>
      </div>
    </main>
  );
}
