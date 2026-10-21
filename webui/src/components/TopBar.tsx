import type { Settings } from "../settings.ts";
import { pollIntervalMs } from "../settings.ts";

interface Props {
  settings: Settings;
  error: string | null;
  updatedAt: Date | null;
  onRefresh: () => void;
}

export function TopBar({ settings, error, updatedAt, onRefresh }: Props) {
  const interval = pollIntervalMs(settings);
  return (
    <header className="top-bar">
      <span className="title">JUIZ</span>
      <span className="field">
        接続先 <code>{settings.serverUrl || window.location.origin}</code>
      </span>
      <span className="field">
        状態
        {error ? (
          <span className="status-error" title={error}>
            未接続
          </span>
        ) : updatedAt ? (
          <span className="status-ok">接続中</span>
        ) : (
          <span>確認中</span>
        )}
      </span>
      <span className="spacer" />
      <span className="field">
        更新間隔 <code>{interval === null ? "停止" : `${interval / 1000} 秒`}</code>
      </span>
      <span className="field">
        最終更新 <code>{updatedAt ? updatedAt.toLocaleTimeString("ja-JP") : "なし"}</code>
      </span>
      <button type="button" onClick={onRefresh}>
        更新
      </button>
    </header>
  );
}
