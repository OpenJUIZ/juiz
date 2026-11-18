import { useCallback, useState } from "react";
import { CenterPane } from "./components/CenterPane.tsx";
import { PropertyPane } from "./components/PropertyPane.tsx";
import { SettingsDialog } from "./components/SettingsDialog.tsx";
import { TopBar } from "./components/TopBar.tsx";
import { TreePane } from "./components/TreePane.tsx";
import { categoryKey, type Model } from "./model.ts";
import { loadSettings, pollIntervalMs, saveSettings, type Settings } from "./settings.ts";
import type { TermKey } from "./terms.ts";
import { useSystemModel } from "./useSystemModel.ts";
import { defaultPaneOpen, useLayoutMode, type LayoutMode } from "./useLayoutMode.ts";

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

  // 左右の欄を出すか。画面の幅で並べ方が変わったら、その並べ方の初めの状態に戻す
  const mode = useLayoutMode();
  const [panes, setPanes] = useState<{ mode: LayoutMode; tree: boolean; props: boolean }>(() => ({
    mode,
    ...defaultPaneOpen(mode),
  }));
  if (panes.mode !== mode) setPanes({ mode, ...defaultPaneOpen(mode) });
  const overlayTree = mode === "narrow";
  const overlayProps = mode !== "wide";
  const togglePane = useCallback(
    (pane: "tree" | "props") =>
      setPanes((p) => {
        const next = { ...p, [pane]: !p[pane] };
        // 狭い画面では引き出す欄は 1 つだけ
        if (p.mode === "narrow" && next[pane]) next[pane === "tree" ? "props" : "tree"] = false;
        return next;
      }),
    [],
  );
  const closeOverlays = useCallback(
    () => setPanes((p) => ({ ...p, tree: overlayTree ? false : p.tree, props: overlayProps ? false : p.props })),
    [overlayTree, overlayProps],
  );
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

  const appClass = [
    "app",
    `layout-${mode}`,
    panes.tree ? "tree-open" : "tree-closed",
    panes.props ? "props-open" : "props-closed",
  ].join(" ");
  const overlayOpen = (overlayTree && panes.tree) || (overlayProps && panes.props);

  return (
    <div className={appClass}>
      <TopBar
        settings={settings}
        error={error}
        updatedAt={updatedAt}
        onRefresh={refresh}
        treeOpen={panes.tree}
        propsOpen={panes.props}
        onTogglePane={togglePane}
      />
      <TreePane
        model={model}
        error={error}
        selectedKey={selectedKey}
        collapsed={collapsed}
        onSelect={(key) => {
          select(key);
          // 引き出したツリーから選んだら、ツリーを閉じて図と性質欄を見せる
          if (overlayTree) setPanes((p) => ({ ...p, tree: false, props: true }));
        }}
        onToggle={toggle}
        onOpenSettings={() => setSettingsOpen(true)}
      />
      <CenterPane
        model={model}
        serverUrl={settings.serverUrl}
        selectedKey={selectedKey}
        onSelect={(key) => (key ? select(key) : setSelectedKey(null))}
        onChanged={refresh}
      />
      {overlayOpen && <div className="pane-backdrop" onClick={closeOverlays} aria-hidden="true" />}
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
