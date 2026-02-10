import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { onOpenUrl } from "@tauri-apps/plugin-deep-link";

import Sidebar from "./components/Sidebar";
import SettingsPanel from "./components/SettingsPanel";
import SyncPanel from "./components/SyncPanel";
import QueryPanel from "./components/QueryPanel";

export default function App() {
  const [activeView, setActiveView] = useState("settings");
  const [config, setConfig] = useState({
    api_base_url: "",
    api_key: "",
    watched_folder: null,
    auto_ingest: true,
    auto_approve_watched: true,
    environment: "Dev",
    session_token: null,
    user_hash: null,
  });
  const [syncStatus, setSyncStatus] = useState({
    watching: false,
    folder: null,
    file_count: 0,
    recent_activity: [],
  });
  const [error, setError] = useState(null);
  const [success, setSuccess] = useState(null);

  const loadState = useCallback(async () => {
    try {
      const [cfg, status] = await Promise.all([
        invoke("get_config"),
        invoke("get_sync_status"),
      ]);
      setConfig(cfg);
      setSyncStatus(status);
      if (status.watching) {
        setActiveView("sync");
      }
    } catch (err) {
      console.error("Failed to load state:", err);
    }
  }, []);

  const handleAuthCallback = useCallback(async (data) => {
    if (!data.api_key || !data.user_hash) return;
    setConfig((prev) => {
      const newConfig = {
        ...prev,
        api_key: data.api_key,
        user_hash: data.user_hash,
        session_token: data.session_token || null,
      };
      invoke("save_config", { newConfig }).catch((err) => setError(String(err)));
      return newConfig;
    });
    setSuccess("Signed in and API key saved.");
    setTimeout(() => setSuccess(null), 3000);
  }, []);

  useEffect(() => {
    loadState();

    const unlistenActivity = listen("sync-activity", (event) => {
      setSyncStatus((prev) => {
        const entry = {
          filename: event.payload.filename,
          status: event.payload.status,
          error: event.payload.error,
          timestamp: String(Math.floor(Date.now() / 1000)),
          category: event.payload.category || null,
        };
        const updated = [entry, ...prev.recent_activity].slice(0, 50);
        return { ...prev, recent_activity: updated };
      });
    });

    const unlistenStatus = listen("sync-status-changed", (event) => {
      setSyncStatus((prev) => ({ ...prev, watching: event.payload }));
      if (event.payload) setActiveView("sync");
    });

    const unlistenTray = listen("tray-toggle-watching", async () => {
      try {
        const status = await invoke("get_sync_status");
        if (status.watching) {
          await invoke("stop_watching");
        } else {
          await invoke("start_watching");
        }
        const newStatus = await invoke("get_sync_status");
        setSyncStatus(newStatus);
      } catch (err) {
        setError(String(err));
      }
    });

    const unlistenDeepLink = listen("deep-link-auth", (event) => {
      handleAuthCallback(event.payload);
    });

    let unlistenDeepLinkJs;
    onOpenUrl((urls) => {
      for (const urlStr of urls) {
        try {
          const url = new URL(urlStr);
          if (url.host === "auth") {
            handleAuthCallback({
              api_key: url.searchParams.get("api_key"),
              user_hash: url.searchParams.get("user_hash"),
              session_token: url.searchParams.get("session_token"),
            });
          }
        } catch (e) {
          console.error("Failed to parse deep link:", e);
        }
      }
    }).then((fn) => {
      unlistenDeepLinkJs = fn;
    }).catch((e) => {
      console.warn("Deep link listener not available:", e);
    });

    return () => {
      unlistenActivity.then((f) => f());
      unlistenStatus.then((f) => f());
      unlistenTray.then((f) => f());
      unlistenDeepLink.then((f) => f());
      if (unlistenDeepLinkJs) unlistenDeepLinkJs();
    };
  }, [loadState, handleAuthCallback]);

  const saveConfig = async (newConfig) => {
    await invoke("save_config", { newConfig });
    setConfig(newConfig);
  };

  const handleScanAndWatch = () => {
    setActiveView("sync");
  };

  return (
    <div className="app-layout">
      <Sidebar activeView={activeView} onViewChange={setActiveView} />

      <div className="content-area">
        <div className="max-w-2xl mx-auto space-y-4">
          {/* Header */}
          <div className="flex items-center justify-between">
            <h1 className="text-xl font-bold text-gray-900">Exemem Client</h1>
          </div>

          {/* Alerts */}
          {error && (
            <div className="bg-red-50 border border-red-200 text-red-700 px-4 py-3 rounded-lg text-sm">
              {error}
              <button onClick={() => setError(null)} className="float-right text-red-400 hover:text-red-600 ml-2">x</button>
            </div>
          )}
          {success && (
            <div className="bg-green-50 border border-green-200 text-green-700 px-4 py-3 rounded-lg text-sm">
              {success}
            </div>
          )}

          {/* View Router */}
          {activeView === "settings" && (
            <SettingsPanel
              config={config}
              setConfig={setConfig}
              saveConfig={saveConfig}
              setError={setError}
              setSuccess={setSuccess}
              onScanAndWatch={handleScanAndWatch}
            />
          )}

          {activeView === "sync" && (
            <SyncPanel
              config={config}
              saveConfig={saveConfig}
              setError={setError}
              setSuccess={setSuccess}
              syncStatus={syncStatus}
              setSyncStatus={setSyncStatus}
            />
          )}

          {activeView === "query" && (
            <QueryPanel
              config={config}
              setError={setError}
            />
          )}
        </div>
      </div>
    </div>
  );
}
