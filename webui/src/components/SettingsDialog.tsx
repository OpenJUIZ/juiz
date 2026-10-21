import { useState } from "react";
import { MIN_POLL_INTERVAL_SEC, type Settings } from "../settings.ts";

interface Props {
  settings: Settings;
  onSave: (s: Settings) => void;
  onClose: () => void;
}

export function SettingsDialog({ settings, onSave, onClose }: Props) {
  const [serverUrl, setServerUrl] = useState(settings.serverUrl);
  const [intervalInput, setIntervalInput] = useState(String(settings.pollIntervalSec));

  const save = () => {
    const n = Number(intervalInput);
    onSave({
      serverUrl: serverUrl.trim(),
      pollIntervalSec: Number.isFinite(n) ? n : settings.pollIntervalSec,
    });
  };

  return (
    <div className="dialog-backdrop" onClick={onClose}>
      <form
        className="dialog"
        onClick={(e) => e.stopPropagation()}
        onSubmit={(e) => {
          e.preventDefault();
          save();
        }}
      >
        <h2>設定</h2>
        <label>
          接続先
          <input
            type="text"
            value={serverUrl}
            placeholder={window.location.origin}
            onChange={(e) => setServerUrl(e.target.value)}
          />
          <span className="hint">空なら画面と同じ場所。別の場所の juiz には CORS の設定が要ります</span>
        </label>
        <label>
          更新間隔(秒)
          <input
            type="number"
            step="0.5"
            value={intervalInput}
            onChange={(e) => setIntervalInput(e.target.value)}
          />
          <span className="hint">
            0 以下で自動更新なし(更新ボタンだけ)。最短 {MIN_POLL_INTERVAL_SEC} 秒
          </span>
        </label>
        <div className="buttons">
          <button type="button" onClick={onClose}>
            取消
          </button>
          <button type="submit">保存</button>
        </div>
      </form>
    </div>
  );
}
