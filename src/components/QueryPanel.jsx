import { useState, useRef, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";

export default function QueryPanel({ config, setError }) {
  const [messages, setMessages] = useState([]);
  const [input, setInput] = useState("");
  const [loading, setLoading] = useState(false);
  const [sessionId, setSessionId] = useState(null);
  const [mode, setMode] = useState("ai"); // "ai" or "search"
  const messagesEndRef = useRef(null);

  const scrollToBottom = () => {
    messagesEndRef.current?.scrollIntoView({ behavior: "smooth" });
  };

  useEffect(() => {
    scrollToBottom();
  }, [messages]);

  const isAuthenticated = !!(config.api_key);

  const handleSubmit = async (e) => {
    e.preventDefault();
    const trimmed = input.trim();
    if (!trimmed || loading) return;

    setInput("");

    if (mode === "search") {
      setMessages((prev) => [...prev, { role: "user", content: trimmed, mode: "search" }]);
      setLoading(true);
      try {
        const resp = await invoke("search_index", { term: trimmed });
        setMessages((prev) => [...prev, {
          role: "assistant",
          content: resp.count > 0
            ? `Found ${resp.count} results for "${resp.term}"`
            : `No results found for "${resp.term}"`,
          data: resp.results,
          mode: "search",
        }]);
      } catch (err) {
        setMessages((prev) => [...prev, { role: "error", content: String(err) }]);
      } finally {
        setLoading(false);
      }
      return;
    }

    // AI Query mode
    if (sessionId) {
      // Follow-up question
      setMessages((prev) => [...prev, { role: "user", content: trimmed, mode: "ai" }]);
      setLoading(true);
      try {
        const resp = await invoke("chat_followup", { sessionId, question: trimmed });
        setMessages((prev) => [...prev, {
          role: "assistant",
          content: resp.answer,
          mode: "ai",
        }]);
      } catch (err) {
        setMessages((prev) => [...prev, { role: "error", content: String(err) }]);
      } finally {
        setLoading(false);
      }
    } else {
      // New query
      setMessages((prev) => [...prev, { role: "user", content: trimmed, mode: "ai" }]);
      setLoading(true);
      try {
        const resp = await invoke("run_query", { query: trimmed, sessionId: null });
        setSessionId(resp.session_id);
        setMessages((prev) => [...prev, {
          role: "assistant",
          content: resp.ai_interpretation || "Query completed.",
          data: resp.raw_results,
          mode: "ai",
        }]);
      } catch (err) {
        setMessages((prev) => [...prev, { role: "error", content: String(err) }]);
      } finally {
        setLoading(false);
      }
    }
  };

  const handleNewSession = () => {
    setSessionId(null);
    setMessages([]);
  };

  const renderData = (data) => {
    if (!data || data.length === 0) return null;

    // Try to render as table if objects
    if (typeof data[0] === "object" && data[0] !== null && !Array.isArray(data[0])) {
      const keys = Object.keys(data[0]);
      return (
        <div className="overflow-x-auto mt-2">
          <table className="min-w-full text-xs border border-gray-200 rounded">
            <thead>
              <tr className="bg-gray-100">
                {keys.map((key) => (
                  <th key={key} className="px-2 py-1 text-left font-medium text-gray-600 border-b border-gray-200">
                    {key}
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {data.slice(0, 50).map((row, i) => (
                <tr key={i} className={i % 2 === 0 ? "bg-white" : "bg-gray-50"}>
                  {keys.map((key) => (
                    <td key={key} className="px-2 py-1 text-gray-700 border-b border-gray-100 max-w-[200px] truncate">
                      {typeof row[key] === "object" ? JSON.stringify(row[key]) : String(row[key] ?? "")}
                    </td>
                  ))}
                </tr>
              ))}
            </tbody>
          </table>
          {data.length > 50 && (
            <p className="text-xs text-gray-400 mt-1">Showing 50 of {data.length} results</p>
          )}
        </div>
      );
    }

    // Fallback: raw JSON
    return (
      <pre className="mt-2 text-xs bg-gray-50 p-2 rounded overflow-x-auto max-h-48 text-gray-700">
        {JSON.stringify(data, null, 2)}
      </pre>
    );
  };

  if (!isAuthenticated) {
    return (
      <div className="bg-white rounded-xl shadow-sm border border-gray-200 p-8 text-center space-y-4">
        <div className="inline-flex items-center justify-center w-12 h-12 bg-gray-100 rounded-full">
          <svg className="w-6 h-6 text-gray-400" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.5}>
            <path strokeLinecap="round" strokeLinejoin="round" d="M16.5 10.5V6.75a4.5 4.5 0 1 0-9 0v3.75m-.75 11.25h10.5a2.25 2.25 0 0 0 2.25-2.25v-6.75a2.25 2.25 0 0 0-2.25-2.25H6.75a2.25 2.25 0 0 0-2.25 2.25v6.75a2.25 2.25 0 0 0 2.25 2.25Z" />
          </svg>
        </div>
        <div>
          <h2 className="text-lg font-semibold text-gray-900">Authentication Required</h2>
          <p className="text-sm text-gray-500 mt-1">Sign in via Settings to use Smart Query.</p>
        </div>
      </div>
    );
  }

  return (
    <div className="query-panel">
      {/* Header */}
      <div className="flex items-center justify-between mb-3">
        <div className="flex items-center gap-2">
          <h2 className="text-sm font-semibold text-gray-700 uppercase tracking-wide">Smart Query</h2>
          {sessionId && (
            <span className="text-xs text-gray-400">Session active</span>
          )}
        </div>
        <div className="flex items-center gap-2">
          {/* Mode toggle */}
          <div className="flex bg-gray-100 rounded-lg p-0.5">
            <button
              onClick={() => setMode("ai")}
              className={`px-2.5 py-1 rounded-md text-xs font-medium transition-colors ${
                mode === "ai" ? "bg-white text-gray-900 shadow-sm" : "text-gray-500"
              }`}
            >
              AI Query
            </button>
            <button
              onClick={() => setMode("search")}
              className={`px-2.5 py-1 rounded-md text-xs font-medium transition-colors ${
                mode === "search" ? "bg-white text-gray-900 shadow-sm" : "text-gray-500"
              }`}
            >
              Index Search
            </button>
          </div>
          {sessionId && (
            <button
              onClick={handleNewSession}
              className="px-2 py-1 text-xs text-gray-500 hover:text-gray-700 border border-gray-200 rounded-lg"
            >
              New Session
            </button>
          )}
        </div>
      </div>

      {/* Message list */}
      <div className="query-messages">
        {messages.length === 0 && (
          <div className="flex-1 flex items-center justify-center">
            <div className="text-center text-gray-400 space-y-2">
              <svg className="w-10 h-10 mx-auto text-gray-300" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1}>
                <path strokeLinecap="round" strokeLinejoin="round" d="M7.5 8.25h9m-9 3H12m-9.75 1.51c0 1.6 1.123 2.994 2.707 3.227 1.129.166 2.27.293 3.423.379.35.026.67.21.865.501L12 21l2.755-4.133a1.14 1.14 0 0 1 .865-.501 48.172 48.172 0 0 0 3.423-.379c1.584-.233 2.707-1.626 2.707-3.228V6.741c0-1.602-1.123-2.995-2.707-3.228A48.394 48.394 0 0 0 12 3c-2.392 0-4.744.175-7.043.513C3.373 3.746 2.25 5.14 2.25 6.741v6.018Z" />
              </svg>
              <p className="text-sm">
                {mode === "ai"
                  ? "Ask a question about your data"
                  : "Search your indexed content"}
              </p>
            </div>
          </div>
        )}

        {messages.map((msg, i) => (
          <div key={i} className={`mb-3 ${msg.role === "user" ? "flex justify-end" : ""}`}>
            {msg.role === "user" && (
              <div className="max-w-[80%] px-3 py-2 bg-primary text-white rounded-xl rounded-br-sm text-sm">
                {msg.content}
              </div>
            )}
            {msg.role === "assistant" && (
              <div className="max-w-[90%] px-3 py-2 bg-white border border-gray-200 rounded-xl rounded-bl-sm shadow-sm">
                <p className="text-sm text-gray-800 whitespace-pre-wrap">{msg.content}</p>
                {renderData(msg.data)}
              </div>
            )}
            {msg.role === "error" && (
              <div className="max-w-[90%] px-3 py-2 bg-red-50 border border-red-200 rounded-xl text-sm text-red-700">
                {msg.content}
              </div>
            )}
          </div>
        ))}

        {loading && (
          <div className="mb-3">
            <div className="inline-flex items-center gap-2 px-3 py-2 bg-white border border-gray-200 rounded-xl shadow-sm">
              <svg className="w-4 h-4 text-gray-400 animate-spin" fill="none" viewBox="0 0 24 24">
                <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4" />
                <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z" />
              </svg>
              <span className="text-sm text-gray-500">Thinking...</span>
            </div>
          </div>
        )}

        <div ref={messagesEndRef} />
      </div>

      {/* Input bar */}
      <form onSubmit={handleSubmit} className="query-input-bar">
        <input
          type="text"
          value={input}
          onChange={(e) => setInput(e.target.value)}
          placeholder={mode === "ai"
            ? (sessionId ? "Ask a follow-up question..." : "Ask about your data...")
            : "Search for a term..."}
          className="flex-1 px-3 py-2 border border-gray-300 rounded-lg text-sm focus:ring-2 focus:ring-primary focus:border-primary"
          disabled={loading}
        />
        <button
          type="submit"
          disabled={loading || !input.trim()}
          className="px-4 py-2 bg-primary text-white rounded-lg text-sm font-medium hover:bg-secondary transition-colors disabled:opacity-50"
        >
          {mode === "ai" ? "Ask" : "Search"}
        </button>
      </form>
    </div>
  );
}
