import { useState } from "react";
import { DiagramPane } from "./components/DiagramPane.tsx";
import { PropertyPane } from "./components/PropertyPane.tsx";
import { SettingsDialog } from "./components/SettingsDialog.tsx";
import { TopBar } from "./components/TopBar.tsx";
import { TreePane } from "./components/TreePane.tsx";
import { loadSettings, pollIntervalMs, saveSettings, type Settings } from "./settings.ts";
import { useSystemProfile } from "./useSystemProfile.ts";

export default function App() {
  const [settings, setSettings] = useState<Settings>(loadSettings);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const { profile, error, updatedAt, refresh } = useSystemProfile(
    settings.serverUrl,
    pollIntervalMs(settings),
  );

  return (
    <div className="app">
      <TopBar
        settings={settings}
        error={error}
        updatedAt={updatedAt}
        onRefresh={refresh}
      />
      <TreePane profile={profile} onOpenSettings={() => setSettingsOpen(true)} />
      <DiagramPane />
      <PropertyPane />
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
