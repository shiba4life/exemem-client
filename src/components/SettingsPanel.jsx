import { open } from "@tauri-apps/plugin-shell";
import { open as openDialog } from "@tauri-apps/plugin-dialog";

const ENV_URLS = {
  Dev: "https://ygyu7ritx8.execute-api.us-west-2.amazonaws.com",
  Prod: "https://jdsx4ixk2i.execute-api.us-east-1.amazonaws.com",
};

const AUTH_PAGE_URLS = {
  Dev: "https://d3t6377alb85xe.cloudfront.net/desktop-auth",
  Prod: "https://exemem.com/desktop-auth",
};

export default function SettingsPanel({
  config,
  setConfig,
  saveConfig,
  setError,
  setSuccess,
  onScanAndWatch,
}) {
  const isAuthenticated = !!(config.session_token && config.user_hash && config.api_key);
  const apiBaseUrl = config.environment === "Custom"
    ? config.api_base_url
    : ENV_URLS[config.environment] || ENV_URLS.Dev;

  const handleSelectFolder = async () => {
    setError(null);
    try {
      const folder = await openDialog({ directory: true, multiple: false });
      if (folder) {
        setConfig((prev) => ({ ...prev, watched_folder: folder }));
      }
    } catch (err) {
      setError("Folder selection failed: " + String(err));
    }
  };

  const handleSignIn = async () => {
    setError(null);
    try {
      const authPageUrl = config.environment === "Custom"
        ? `${apiBaseUrl}/desktop-auth`
        : AUTH_PAGE_URLS[config.environment] || AUTH_PAGE_URLS.Dev;
      const authUrl = `${authPageUrl}?api=${encodeURIComponent(apiBaseUrl)}`;
      await open(authUrl);
      setSuccess("Browser opened. Complete sign-in there, then click \"Open Exemem Client\" to return.");
      setTimeout(() => setSuccess(null), 10000);
    } catch (err) {
      setError(String(err));
    }
  };

  const handleSignOut = async () => {
    const newConfig = { ...config, api_key: "", session_token: null, user_hash: null };
    await saveConfig(newConfig);
    setSuccess("Signed out.");
    setTimeout(() => setSuccess(null), 3000);
  };

  const handleSave = async () => {
    setError(null);
    setSuccess(null);
    try {
      await saveConfig(config);
      setSuccess("Configuration saved.");
      setTimeout(() => setSuccess(null), 3000);
    } catch (err) {
      setError(String(err));
    }
  };

  return (
    <div className="bg-white rounded-xl shadow-sm border border-gray-200 p-5 space-y-4">
      <h2 className="text-sm font-semibold text-gray-700 uppercase tracking-wide">Settings</h2>

      <div>
        <label className="block text-sm font-medium text-gray-700 mb-1">Environment</label>
        <select
          className="w-full px-3 py-2 border border-gray-300 rounded-lg text-sm focus:ring-2 focus:ring-primary focus:border-primary bg-white"
          value={config.environment}
          onChange={(e) => setConfig((prev) => ({ ...prev, environment: e.target.value }))}
        >
          <option value="Dev">Dev</option>
          <option value="Prod">Prod</option>
          <option value="Custom">Custom</option>
        </select>
        {config.environment === "Custom" ? (
          <input
            type="text"
            className="w-full mt-2 px-3 py-2 border border-gray-300 rounded-lg text-sm focus:ring-2 focus:ring-primary focus:border-primary"
            placeholder="https://your-api.example.com"
            value={config.api_base_url}
            onChange={(e) => setConfig((prev) => ({ ...prev, api_base_url: e.target.value }))}
          />
        ) : (
          <p className="mt-1 text-xs text-gray-500">{apiBaseUrl}</p>
        )}
      </div>

      {/* Authentication */}
      <div>
        <label className="block text-sm font-medium text-gray-700 mb-1">Authentication</label>
        {isAuthenticated ? (
          <div className="space-y-2">
            <div className="flex items-center justify-between px-3 py-2 bg-green-50 border border-green-200 rounded-lg">
              <div className="flex items-center gap-2">
                <span className="w-2 h-2 rounded-full bg-green-500" />
                <span className="text-sm text-green-700">Signed in as {config.user_hash.slice(0, 12)}...</span>
              </div>
              <button onClick={handleSignOut} className="text-xs text-gray-500 hover:text-red-600 transition-colors">Sign Out</button>
            </div>
            <div>
              <label className="block text-xs text-gray-500 mb-1">API Key</label>
              <input type="password" className="w-full px-3 py-2 border border-gray-300 rounded-lg text-sm bg-gray-50" value={config.api_key} readOnly />
            </div>
          </div>
        ) : (
          <div className="space-y-2">
            <button onClick={handleSignIn} className="w-full px-4 py-2 bg-indigo-600 text-white rounded-lg text-sm font-medium hover:bg-indigo-700 transition-colors">
              Sign In with Passkey
            </button>
            <div>
              <label className="block text-xs text-gray-500 mb-1">Or enter API key manually</label>
              <input
                type="password"
                className="w-full px-3 py-2 border border-gray-300 rounded-lg text-sm focus:ring-2 focus:ring-primary focus:border-primary"
                placeholder="Enter your API key"
                value={config.api_key}
                onChange={(e) => setConfig((prev) => ({ ...prev, api_key: e.target.value }))}
              />
            </div>
          </div>
        )}
      </div>

      <div>
        <label className="block text-sm font-medium text-gray-700 mb-1">Watched Folder</label>
        <div className="flex gap-2">
          <input
            type="text"
            className="flex-1 px-3 py-2 border border-gray-300 rounded-lg text-sm focus:ring-2 focus:ring-primary focus:border-primary"
            placeholder="/path/to/folder"
            value={config.watched_folder || ""}
            onChange={(e) => setConfig((prev) => ({ ...prev, watched_folder: e.target.value || null }))}
          />
          <button onClick={handleSelectFolder} className="px-4 py-2 bg-gray-100 text-gray-700 rounded-lg text-sm font-medium hover:bg-gray-200 transition-colors">Browse</button>
        </div>
      </div>

      <div className="flex items-center justify-between">
        <label className="text-sm font-medium text-gray-700">Auto-ingest after upload</label>
        <button
          onClick={() => setConfig((prev) => ({ ...prev, auto_ingest: !prev.auto_ingest }))}
          className={`relative inline-flex h-6 w-11 items-center rounded-full transition-colors ${config.auto_ingest ? "bg-primary" : "bg-gray-300"}`}
        >
          <span className={`inline-block h-4 w-4 transform rounded-full bg-white transition-transform ${config.auto_ingest ? "translate-x-6" : "translate-x-1"}`} />
        </button>
      </div>

      <div className="flex items-center justify-between">
        <label className="text-sm font-medium text-gray-700">Auto-approve watched files</label>
        <button
          onClick={() => setConfig((prev) => ({ ...prev, auto_approve_watched: !prev.auto_approve_watched }))}
          className={`relative inline-flex h-6 w-11 items-center rounded-full transition-colors ${config.auto_approve_watched ? "bg-primary" : "bg-gray-300"}`}
        >
          <span className={`inline-block h-4 w-4 transform rounded-full bg-white transition-transform ${config.auto_approve_watched ? "translate-x-6" : "translate-x-1"}`} />
        </button>
      </div>

      <div className="flex gap-2 pt-2">
        <button onClick={handleSave} className="flex-1 px-4 py-2 bg-gray-200 text-gray-700 rounded-lg text-sm font-medium hover:bg-gray-300 transition-colors">
          Save Settings
        </button>
        <button onClick={onScanAndWatch} className="flex-1 px-4 py-2 bg-primary text-white rounded-lg text-sm font-medium hover:bg-secondary transition-colors">
          Scan & Watch
        </button>
      </div>
    </div>
  );
}
