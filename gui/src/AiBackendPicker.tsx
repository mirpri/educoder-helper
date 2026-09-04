// 「由谁来写」的选择器：API / 本地 claude / 本地 codex 三选一，及各自的设置表单。
// 实验报告页和 AI 做实验页共用，避免同一套输入框写两遍。
import { open } from "@tauri-apps/plugin-dialog";
import { Folder } from "lucide-react";

import type { BackendConfig, BackendKind, DetectedClis } from "./types";
import { Field } from "./ui";

/** 常见服务商的地址与模型，省得用户去翻文档。 */
export const PRESETS: { label: string; baseUrl: string; model: string }[] = [
  { label: "DeepSeek", baseUrl: "https://api.deepseek.com", model: "deepseek-chat" },
  {
    label: "通义千问",
    baseUrl: "https://dashscope.aliyuncs.com/compatible-mode/v1",
    model: "qwen-plus",
  },
  { label: "Kimi（月之暗面）", baseUrl: "https://api.moonshot.cn/v1", model: "moonshot-v1-32k" },
  { label: "智谱 GLM", baseUrl: "https://open.bigmodel.cn/api/paas/v4", model: "glm-4-plus" },
  { label: "硅基流动", baseUrl: "https://api.siliconflow.cn/v1", model: "deepseek-ai/DeepSeek-V3" },
  { label: "OpenAI", baseUrl: "https://api.openai.com/v1", model: "gpt-4o" },
];

export const BACKENDS: { value: BackendKind; label: string; blurb: string }[] = [
  {
    value: "api",
    label: "API（OpenAI 兼容）",
    blurb: "填自己的 API Key，任何 OpenAI 兼容接口都行。最通用，按 token 付费。",
  },
  {
    value: "claudeCode",
    label: "Claude Code（本地）",
    blurb: "调用本机已安装并登录的 claude 命令，不需要 API Key。",
  },
  {
    value: "codex",
    label: "Codex（本地）",
    blurb: "调用本机已安装并登录的 codex 命令，不需要 API Key。",
  },
];

/** 当前后端实际会用哪个可执行文件：手填优先，其次是探测到的。 */
export function cliPath(backend: BackendConfig, detected: DetectedClis | null): string | null {
  if (backend.cli.path.trim()) return backend.cli.path.trim();
  if (backend.kind === "claudeCode") return detected?.claudeCode ?? null;
  if (backend.kind === "codex") return detected?.codex ?? null;
  return null;
}

/** 走本地 CLI 用你已登录的账号，不需要 key；只有 API 后端才必须填。 */
export function backendReady(backend: BackendConfig, detected: DetectedClis | null): boolean {
  return backend.kind === "api"
    ? backend.api.apiKey.trim() !== ""
    : cliPath(backend, detected) !== null;
}

export function AiBackendPicker({
  backend,
  setBackend,
  remember,
  setRemember,
  running,
  detected,
}: {
  backend: BackendConfig;
  setBackend: (updater: (prev: BackendConfig) => BackendConfig) => void;
  remember: boolean;
  setRemember: (v: boolean) => void;
  running: boolean;
  detected: DetectedClis | null;
}) {
  const setApi = (patch: Partial<BackendConfig["api"]>) =>
    setBackend((prev) => ({ ...prev, api: { ...prev.api, ...patch } }));
  const setCli = (patch: Partial<BackendConfig["cli"]>) =>
    setBackend((prev) => ({ ...prev, cli: { ...prev.cli, ...patch } }));

  return (
    <>
      <div className="segmented">
        {BACKENDS.map((b) => (
          <button
            key={b.value}
            type="button"
            className={`segment${backend.kind === b.value ? " is-active" : ""}`}
            onClick={() => setBackend((prev) => ({ ...prev, kind: b.value }))}
            disabled={running}
          >
            {b.label}
          </button>
        ))}
      </div>
      <p className="muted small">{BACKENDS.find((b) => b.value === backend.kind)!.blurb}</p>

      {backend.kind === "api" ? (
        <div className="form-grid">
          <Field label="服务商预设" hint="选一个自动填好地址和模型，也可以手动改。">
            <select
              className="input select"
              value={PRESETS.find((p) => p.baseUrl === backend.api.baseUrl)?.label ?? ""}
              onChange={(e) => {
                const p = PRESETS.find((x) => x.label === e.target.value);
                if (p) setApi({ baseUrl: p.baseUrl, model: p.model });
              }}
              disabled={running}
            >
              <option value="">自定义</option>
              {PRESETS.map((p) => (
                <option key={p.label} value={p.label}>
                  {p.label}
                </option>
              ))}
            </select>
          </Field>

          <Field label="API 地址" hint="填到 /v1 或域名都行，会自动补 /chat/completions。">
            <input
              className="input mono"
              value={backend.api.baseUrl}
              onChange={(e) => setApi({ baseUrl: e.target.value })}
              disabled={running}
              spellCheck={false}
            />
          </Field>

          <Field label="模型">
            <input
              className="input mono"
              value={backend.api.model}
              onChange={(e) => setApi({ model: e.target.value })}
              disabled={running}
              spellCheck={false}
            />
          </Field>

          <Field label="API Key" hint="只发往你填的 API 地址，不会上传到别处。">
            <input
              className="input mono"
              type="password"
              value={backend.api.apiKey}
              onChange={(e) => setApi({ apiKey: e.target.value })}
              placeholder="sk-…"
              disabled={running}
              spellCheck={false}
            />
          </Field>

          <label className="check">
            <input
              type="checkbox"
              checked={remember}
              onChange={(e) => setRemember(e.target.checked)}
              disabled={running}
            />
            <span>
              记住 API Key
              <span className="muted small block">
                以明文存进应用配置目录的 config.json。不勾则只保留在本次运行期间。
              </span>
            </span>
          </label>
        </div>
      ) : (
        <div className="form-grid">
          <div className="notice">
            <span className="notice-body">
              {cliPath(backend, detected) ? (
                <>
                  已检测到：
                  <span className="mono">{cliPath(backend, detected)}</span>
                </>
              ) : (
                <>
                  没在 PATH 上找到{" "}
                  <span className="mono">{backend.kind === "claudeCode" ? "claude" : "codex"}</span>
                  ，请在下面填写完整路径。
                </>
              )}
            </span>
          </div>

          <Field
            label="可执行文件路径（可选）"
            hint="留空则在 PATH 上查找。从文件管理器启动应用时 PATH 可能不全，这时手动填。"
          >
            <div className="input-row">
              <input
                className="input mono"
                value={backend.cli.path}
                onChange={(e) => setCli({ path: e.target.value })}
                placeholder={cliPath(backend, detected) ?? "例如 C:\\Users\\你\\.local\\bin\\claude.exe"}
                disabled={running}
                spellCheck={false}
              />
              <button
                className="btn"
                onClick={() =>
                  void open({ multiple: false, title: "选择可执行文件" }).then((f) => {
                    if (typeof f === "string") setCli({ path: f });
                  })
                }
                disabled={running}
              >
                <Folder size={14} /> 浏览…
              </button>
            </div>
          </Field>

          <Field label="模型（可选）" hint="留空则用该 CLI 自己的默认模型。">
            <input
              className="input mono"
              value={backend.cli.model}
              onChange={(e) => setCli({ model: e.target.value })}
              placeholder={backend.kind === "claudeCode" ? "例如 sonnet" : "例如 gpt-5"}
              disabled={running}
              spellCheck={false}
            />
          </Field>

          <p className="muted small">
            走本地 CLI 用的是你已登录的账号，不需要 API Key。注意每次调用都会带上该 CLI
            自身的系统提示与工具定义（实测约 15k~33k token 的固定开销）——订阅制用户只是消耗额度，
            但按 token 计费的用户会比直连 API 更贵。生成期间工具已被禁用，它不会去读写你的文件。
          </p>
        </div>
      )}
    </>
  );
}
