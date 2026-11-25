// この機械で動いている系の一覧(docs/spec/naming.md §9.4、#618)。選ぶとその系の管理画面を別の画面で開く。
// 左のツリーの「ほかの系」から開く。
import { useEffect, useState } from "react";
import { JuizClient } from "../api/client.ts";
import type { NameRegistration } from "../api/types.ts";
import { registeredName, sortRegistrations, webUiUrl } from "../naming.ts";

interface Props {
  serverUrl: string;
  /** いま見ている系の UUID。一覧で印を付ける */
  currentUuid: string | null;
  onClose: () => void;
}

export function NamesDialog({ serverUrl, currentUuid, onClose }: Props) {
  const [list, setList] = useState<NameRegistration[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const abort = new AbortController();
    new JuizClient(serverUrl)
      .nameList(true, abort.signal)
      .then((l) => setList(sortRegistrations(l)))
      .catch((e: unknown) => {
        if (e instanceof DOMException && e.name === "AbortError") return;
        setError(e instanceof Error ? e.message : String(e));
      });
    return () => abort.abort();
  }, [serverUrl]);

  return (
    <div className="dialog-backdrop" onClick={onClose}>
      <div className="dialog names-dialog" onClick={(e) => e.stopPropagation()}>
        <h2>このマシンで動いている系</h2>
        {error ? (
          <div className="tree-error" role="alert">
            <div className="title">系の名前を取得できません</div>
            <div className="detail">{error}</div>
          </div>
        ) : list === null ? (
          <div className="placeholder">取得しています</div>
        ) : list.length === 0 ? (
          <div className="placeholder">名前で引ける系がありません</div>
        ) : (
          <ul className="names-list">
            {list.map((r) => (
              <NameRow key={r.uuid} r={r} serverUrl={serverUrl} current={r.uuid === currentUuid} />
            ))}
          </ul>
        )}
        <span className="hint">juiz name list と同じ内容です。HTTP の仲介がある系だけ開けます</span>
        <div className="buttons">
          <button type="button" onClick={onClose}>
            やめる
          </button>
        </div>
      </div>
    </div>
  );
}

function NameRow({ r, serverUrl, current }: { r: NameRegistration; serverUrl: string; current: boolean }) {
  const url = webUiUrl(r, serverUrl, window.location.origin);
  const pkg = r.package?.name ? `${r.package.name}${r.package.version ? ` ${r.package.version}` : ""}` : null;
  return (
    <li className={`names-row${r.alive === false ? " silent" : ""}`}>
      <div className="names-main">
        <code className="names-name">{registeredName(r)}</code>
        {pkg && <span className="names-pkg">{pkg}</span>}
      </div>
      <div className="names-detail">
        pid {r.pid} ・ {r.brokers.map((b) => b.type_name).join(" ") || "仲介なし"}
        {r.alive === false && " ・ 応じない"}
      </div>
      <div className="names-action">
        {current ? (
          <span className="hint">今見ている系</span>
        ) : url ? (
          <a href={url} target="_blank" rel="noreferrer">
            開く
          </a>
        ) : (
          <span className="hint">HTTP なし</span>
        )}
      </div>
    </li>
  );
}
