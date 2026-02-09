import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

function StatusBadge({ watching }) {
  return (
    <span
      className={`inline-flex items-center gap-1.5 px-2.5 py-1 rounded-full text-xs font-medium ${
        watching
          ? "bg-green-100 text-green-700"
          : "bg-gray-100 text-gray-600"
      }`}
    >
      <span
        className={`w-2 h-2 rounded-full ${
          watching ? "bg-green-500 animate-pulse" : "bg-gray-400"
        }`}
      />
      {watching ? "Watching" : "Paused"}
    </span>
  );
}

function StatusIcon({ status }) {
  switch (status) {
    case "Uploading":
      return <span className="text-blue-500">...</span>;
    case "Uploaded":
      return <span className="text-blue-600">^</span>;
    case "Ingesting":
      return <span className="text-yellow-500">~</span>;
    case "Done":
      return <span className="text-green-600">ok</span>;
    case "Error":
      return <span className="text-red-500">!</span>;
    default:
      return <span className="text-gray-400">?</span>;
  }
}

export default function App() {
  const [config, setConfig] = useState({
    api_base_url: "",
    api_key: "",
    watched_folder: null,
    auto_ingest: true,
  });
  const [syncStatus, setSyncStatus] = useState({
    watching: false,
    folder: null,
    file_count: 0,
    recent_activity: [],
  });
  const [saving, setSaving] = useState(false);
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
    } catch (err) {
      console.error("Failed to load state:", err);
    }
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
        };
        const updated = [entry, ...prev.recent_activity].slice(0, 50);
        return { ...prev, recent_activity: updated };
      });
    });

    const unlistenStatus = listen("sync-status-changed", (event) => {
      setSyncStatus((prev) => ({ ...prev, watching: event.payload }));
    });

    const unlistenTray = listen("tray-toggle-watching", () => {
      toggleWatching();
    });

    return () => {
      unlistenActivity.then((f) => f());
      unlistenStatus.then((f) => f());
      unlistenTray.then((f) => f());
    };
  }, [loadState]);

  const handleSave = async () => {
    setSaving(true);
    setError(null);
    setSuccess(null);
    try {
      await invoke("save_config", { newConfig: config });
      setSuccess("Configuration saved.");
      setTimeout(() => setSuccess(null), 3000);
    } catch (err) {
      setError(String(err));
    } finally {
      setSaving(false);
    }
  };

  const handleSelectFolder = async () => {
    try {
      const folder = await invoke("select_folder");
      if (folder) {
        setConfig((prev) => ({ ...prev, watched_folder: folder }));
      }
    } catch (err) {
      console.error("Folder selection error:", err);
    }
  };

  const toggleWatching = async () => {
    setError(null);
    try {
      if (syncStatus.watching) {
        await invoke("stop_watching");
      } else {
        await invoke("start_watching");
      }
      const status = await invoke("get_sync_status");
      setSyncStatus(status);
    } catch (err) {
      setError(String(err));
    }
  };

  const formatTime = (timestamp) => {
    if (!timestamp) return "";
    const date = new Date(Number(timestamp) * 1000);
    return date.toLocaleTimeString();
  };

  return (
    <div className="min-h-screen bg-gray-50 p-6">
      <div className="max-w-lg mx-auto space-y-6">
        {/* Header */}
        <div className="flex items-center justify-between">
          <h1 className="text-xl font-bold text-gray-900">Exemem Client</h1>
          <StatusBadge watching={syncStatus.watching} />
        </div>

        {/* Alerts */}
        {error && (
          <div className="bg-red-50 border border-red-200 text-red-700 px-4 py-3 rounded-lg text-sm">
            {error}
          </div>
        )}
        {success && (
          <div className="bg-green-50 border border-green-200 text-green-700 px-4 py-3 rounded-lg text-sm">
            {success}
          </div>
        )}

        {/* Settings */}
        <div className="bg-white rounded-xl shadow-sm border border-gray-200 p-5 space-y-4">
          <h2 className="text-sm font-semibold text-gray-700 uppercase tracking-wide">
            Settings
          </h2>

          <div>
            <label className="block text-sm font-medium text-gray-700 mb-1">
              API URL
            </label>
            <input
              type="text"
              className="w-full px-3 py-2 border border-gray-300 rounded-lg text-sm focus:ring-2 focus:ring-primary focus:border-primary"
              placeholder="https://your-api.example.com/api"
              value={config.api_base_url}
              onChange={(e) =>
                setConfig((prev) => ({
                  ...prev,
                  api_base_url: e.target.value,
                }))
              }
            />
          </div>

          <div>
            <label className="block text-sm font-medium text-gray-700 mb-1">
              API Key
            </label>
            <input
              type="password"
              className="w-full px-3 py-2 border border-gray-300 rounded-lg text-sm focus:ring-2 focus:ring-primary focus:border-primary"
              placeholder="Enter your API key"
              value={config.api_key}
              onChange={(e) =>
                setConfig((prev) => ({ ...prev, api_key: e.target.value }))
              }
            />
          </div>

          <div>
            <label className="block text-sm font-medium text-gray-700 mb-1">
              Watched Folder
            </label>
            <div className="flex gap-2">
              <input
                type="text"
                className="flex-1 px-3 py-2 border border-gray-300 rounded-lg text-sm bg-gray-50"
                placeholder="Select a folder..."
                value={config.watched_folder || ""}
                readOnly
              />
              <button
                onClick={handleSelectFolder}
                className="px-4 py-2 bg-gray-100 text-gray-700 rounded-lg text-sm font-medium hover:bg-gray-200 transition-colors"
              >
                Browse
              </button>
            </div>
          </div>

          <div className="flex items-center justify-between">
            <label className="text-sm font-medium text-gray-700">
              Auto-ingest after upload
            </label>
            <button
              onClick={() =>
                setConfig((prev) => ({
                  ...prev,
                  auto_ingest: !prev.auto_ingest,
                }))
              }
              className={`relative inline-flex h-6 w-11 items-center rounded-full transition-colors ${
                config.auto_ingest ? "bg-primary" : "bg-gray-300"
              }`}
            >
              <span
                className={`inline-block h-4 w-4 transform rounded-full bg-white transition-transform ${
                  config.auto_ingest ? "translate-x-6" : "translate-x-1"
                }`}
              />
            </button>
          </div>

          <div className="flex gap-2 pt-2">
            <button
              onClick={handleSave}
              disabled={saving}
              className="flex-1 px-4 py-2 bg-primary text-white rounded-lg text-sm font-medium hover:bg-secondary transition-colors disabled:opacity-50"
            >
              {saving ? "Saving..." : "Save Settings"}
            </button>
            <button
              onClick={toggleWatching}
              className={`flex-1 px-4 py-2 rounded-lg text-sm font-medium transition-colors ${
                syncStatus.watching
                  ? "bg-red-50 text-red-700 hover:bg-red-100"
                  : "bg-green-50 text-green-700 hover:bg-green-100"
              }`}
            >
              {syncStatus.watching ? "Stop Watching" : "Start Watching"}
            </button>
          </div>
        </div>

        {/* Status */}
        <div className="bg-white rounded-xl shadow-sm border border-gray-200 p-5 space-y-4">
          <div className="flex items-center justify-between">
            <h2 className="text-sm font-semibold text-gray-700 uppercase tracking-wide">
              Activity
            </h2>
            {syncStatus.folder && (
              <span className="text-xs text-gray-500">
                {syncStatus.file_count} files in folder
              </span>
            )}
          </div>

          {syncStatus.recent_activity.length === 0 ? (
            <p className="text-sm text-gray-400 text-center py-6">
              No activity yet. Start watching a folder to see uploads here.
            </p>
          ) : (
            <div className="space-y-2 max-h-80 overflow-y-auto">
              {syncStatus.recent_activity.map((entry, i) => (
                <div
                  key={`${entry.filename}-${entry.timestamp}-${i}`}
                  className="flex items-center gap-3 px-3 py-2 bg-gray-50 rounded-lg"
                >
                  <StatusIcon status={entry.status} />
                  <div className="flex-1 min-w-0">
                    <p className="text-sm font-medium text-gray-800 truncate">
                      {entry.filename}
                    </p>
                    {entry.error && (
                      <p className="text-xs text-red-500 truncate">
                        {entry.error}
                      </p>
                    )}
                  </div>
                  <span className="text-xs text-gray-400 whitespace-nowrap">
                    {formatTime(entry.timestamp)}
                  </span>
                </div>
              ))}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
