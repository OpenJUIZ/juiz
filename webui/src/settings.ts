// 画面の設定。ブラウザの localStorage に保存する(#111)。
// localStorage はオリジン(host:port)ごとなので、juiz ごとに別の設定になる。

export interface Settings {
  /** 接続先。空なら画面と同じオリジン */
  serverUrl: string;
  /** 更新間隔(秒)。0 以下で自動更新なし */
  pollIntervalSec: number;
}

export const DEFAULT_SETTINGS: Settings = {
  serverUrl: "",
  pollIntervalSec: 2,
};

/** 自動更新の間隔の下限(秒) */
export const MIN_POLL_INTERVAL_SEC = 0.5;

const STORAGE_KEY = "juiz-webui.settings";

/** 更新間隔をミリ秒に直す。自動更新しないときは null */
export function pollIntervalMs(s: Settings): number | null {
  if (!(s.pollIntervalSec > 0)) return null;
  return Math.max(s.pollIntervalSec, MIN_POLL_INTERVAL_SEC) * 1000;
}

export function loadSettings(): Settings {
  try {
    const raw = window.localStorage.getItem(STORAGE_KEY);
    if (!raw) return DEFAULT_SETTINGS;
    const v = JSON.parse(raw) as Partial<Settings>;
    return {
      serverUrl: typeof v.serverUrl === "string" ? v.serverUrl : DEFAULT_SETTINGS.serverUrl,
      pollIntervalSec:
        typeof v.pollIntervalSec === "number" && Number.isFinite(v.pollIntervalSec)
          ? v.pollIntervalSec
          : DEFAULT_SETTINGS.pollIntervalSec,
    };
  } catch {
    return DEFAULT_SETTINGS;
  }
}

export function saveSettings(s: Settings): void {
  try {
    window.localStorage.setItem(STORAGE_KEY, JSON.stringify(s));
  } catch {
    // 保存できない環境(プライベートウィンドウなど)では今回だけの設定になる
  }
}
