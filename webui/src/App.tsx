import { useCallback, useState } from "react";
import { DiagramPane } from "./components/DiagramPane.tsx";
import { PropertyPane } from "./components/PropertyPane.tsx";
import { SettingsDialog } from "./components/SettingsDialog.tsx";
import { TopBar } from "./components/TopBar.tsx";
import { TreePane } from "./components/TreePane.tsx";
import { categoryKey, type Model } from "./model.ts";
import { loadSettings, pollIntervalMs, saveSettings, type Settings } from "./settings.ts";
import type { TermKey } from "./terms.ts";
import { useSystemModel } from "./useSystemModel.ts";

/** ツリーでその要素を見せるために開いておく節の鍵 */
function ancestorKeys(model: Model, key: string): string[] {
  const e = model.elements.get(key);
  if (!e) return [];
  const keys: string[] = [];
  const term: TermKey | null =
    e.kind === "system"
      ? null
      : e.kind === "containerProcess"
        ? e.containerKey
          ? "container"
          : "containerProcess"
        : e.kind;
  if (term) keys.push(e.systemKey, categoryKey(e.systemKey, term));
  if (e.kind === "containerProcess" && e.containerKey) keys.push(e.containerKey);
  // 副系なら親の系をたどる(副系の鍵は「親の鍵.UUID」)
  for (let sys = e.systemKey, i = sys.lastIndexOf("."); i >= 0; i = sys.lastIndexOf(".")) {
    sys = sys.slice(0, i);
    keys.push(sys, categoryKey(sys, "subsystem"));
  }
  return keys;
}

export default function App() {
  const [settings, setSettings] = useState<Settings>(loadSettings);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [selectedKey, setSelectedKey] = useState<string | null>(null);
  const [collapsed, setCollapsed] = useState<ReadonlySet<string>>(new Set());
  const { model, error, updatedAt, refresh } = useSystemModel(
    settings.serverUrl,
    pollIntervalMs(settings),
  );

  const toggle = useCallback((key: string) => {
    setCollapsed((prev) => {
      const next = new Set(prev);
      if (!next.delete(key)) next.add(key);
      return next;
    });
  }, []);

  /** 要素を選ぶ。ツリーで畳まれていたら開く */
  const select = useCallback(
    (key: string) => {
      setSelectedKey(key);
      if (!model) return;
      const keys = ancestorKeys(model, key);
      setCollapsed((prev) => (keys.some((k) => prev.has(k)) ? new Set([...prev].filter((k) => !keys.includes(k))) : prev));
    },
    [model],
  );

  return (
    <div className="app">
      <TopBar
        settings={settings}
        error={error}
        updatedAt={updatedAt}
        onRefresh={refresh}
      />
      <TreePane
        model={model}
        error={error}
        selectedKey={selectedKey}
        collapsed={collapsed}
        onSelect={select}
        onToggle={toggle}
        onOpenSettings={() => setSettingsOpen(true)}
      />
      <DiagramPane
        model={model}
        serverUrl={settings.serverUrl}
        selectedKey={selectedKey}
        onSelect={(key) => (key ? select(key) : setSelectedKey(null))}
      />
      <PropertyPane model={model} selectedKey={selectedKey} onSelect={select} serverUrl={settings.serverUrl} />
      {settingsOpen && (
        <SettingsDialog
          settings={settings}
          onSave={(s) => {
            saveSettings(s);
            setSettings(s);
            setSettingsOpen(false);
          }}
          onClose={() => setSettingsOpen(false)}
        />
      )}
    </div>
  );
}
