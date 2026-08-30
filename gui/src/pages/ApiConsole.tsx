// API 页：对应 `edu get / post / raw` —— 对任意接口发起已签名的请求。
import { useState } from "react";

import { Braces, Copy, Send } from "lucide-react";

import * as api from "../api";
import { useApp } from "../context";
import type { RawResponse } from "../types";
import { Badge, ErrorBox, Field, Spinner } from "../ui";

const METHODS = ["GET", "POST", "PUT", "DELETE", "PATCH"];

const SAMPLES = [
  "/api/users/get_user_info.json",
  "/api/courses/<courseId>/homework_commons.json?type=4&page=1&limit=50",
  "/api/shixuns/<shixunId>/challenges.json",
  "/api/tasks/<gameId>.json",
];

export default function ApiConsole() {
  const { goto } = useApp();
  const [method, setMethod] = useState("GET");
  const [path, setPath] = useState("/api/users/get_user_info.json");
  const [body, setBody] = useState("");
  const [busy, setBusy] = useState(false);
  const [resp, setResp] = useState<RawResponse | null>(null);
  const [error, setError] = useState<api.ApiError | null>(null);
  const [pretty, setPretty] = useState(true);

  async function send() {
    setBusy(true);
    setError(null);
    setResp(null);
    try {
      setResp(await api.rawRequest(method, path.trim(), body));
    } catch (e) {
      setError(api.toApiError(e));
    } finally {
      setBusy(false);
    }
  }

  // The raw body is authoritative; pretty view is best-effort JSON formatting.
  const shown = (() => {
    if (!resp) return "";
    if (!pretty) return resp.body;
    try {
      return JSON.stringify(JSON.parse(resp.body), null, 2);
    } catch {
      return resp.body;
    }
  })();

  return (
    <div className="page">
      <header className="page-head">
        <h1>API 调试</h1>
        <p className="muted">
          请求会自动带上 Cookie、与服务器时钟对齐的时间戳和签名 —— 等同于 CLI 的{" "}
          <span className="mono">edu raw &lt;METHOD&gt; &lt;path&gt;</span>。响应原样返回，不做解析。
        </p>
      </header>

      <section className="card">
        <div className="form-grid">
          <div className="input-row">
            <select className="input select" value={method} onChange={(e) => setMethod(e.target.value)}>
              {METHODS.map((m) => (
                <option key={m}>{m}</option>
              ))}
            </select>
            <input
              className="input mono"
              value={path}
              onChange={(e) => setPath(e.target.value)}
              onKeyDown={(e) => e.key === "Enter" && void send()}
              placeholder="/api/... 或完整 https:// 地址"
              spellCheck={false}
            />
            <button className="btn btn-primary" onClick={() => void send()} disabled={busy || !path.trim()}>
              {busy ? <Spinner /> : <Send size={14} />} 发送
            </button>
          </div>

          {method !== "GET" && method !== "DELETE" ? (
            <Field label="请求体" hint="留空则不发送请求体；有内容时以 application/json 提交。">
              <textarea
                className="input textarea mono"
                rows={5}
                value={body}
                onChange={(e) => setBody(e.target.value)}
                placeholder='{"field": "value"}'
                spellCheck={false}
              />
            </Field>
          ) : null}
        </div>

        <div className="samples">
          <span className="muted small">示例：</span>
          {SAMPLES.map((s) => (
            <button key={s} className="linkbtn mono small" onClick={() => setPath(s)}>
              {s}
            </button>
          ))}
        </div>
      </section>

      {error ? <ErrorBox error={error} onFixCookies={() => goto("account")} /> : null}

      {resp ? (
        <section className="card">
          <div className="card-head">
            <h2>响应</h2>
            <div className="row-actions">
              <Badge tone={resp.status < 400 ? "ok" : "error"}>HTTP {resp.status}</Badge>
              {resp.redirected ? <Badge tone="warn">已重定向</Badge> : null}
              <button className="btn btn-ghost btn-sm" onClick={() => setPretty((v) => !v)}>
                <Braces size={13} /> {pretty ? "原始文本" : "格式化"}
              </button>
              <button className="btn btn-ghost btn-sm" onClick={() => void navigator.clipboard.writeText(resp.body)}>
                <Copy size={13} /> 复制
              </button>
            </div>
          </div>
          <p className="mono wrap small muted">{resp.url}</p>
          <pre className="code-block code-file">{shown}</pre>
        </section>
      ) : null}
    </div>
  );
}
