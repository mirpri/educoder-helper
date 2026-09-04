// 实验报告页：勾选整门课程里要写进报告的关卡，粘贴任务书 / 实验要求 / 报告模板，
// 交给 AI 逐个实训起草，拼成一份 报告.md，并把每关的题目与代码存成素材文件。
import { useEffect, useMemo, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { Folder, FolderOpen, ListTree, Sparkles, X } from "lucide-react";

import * as api from "../api";
import { ChallengeTree, allGameIds, selectedHomeworks } from "../ChallengeTree";
import { useApp } from "../context";
import { useTasks } from "../tasks";
import type {
  BackendConfig,
  BackendKind,
  DetectedClis,
  ReportResult,
  ReportTree,
} from "../types";
import { Badge, Empty, ErrorBox, Field, Spinner } from "../ui";

/** 常见服务商的地址与模型，省得用户去翻文档。 */
const PRESETS: { label: string; baseUrl: string; model: string }[] = [
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

const BACKENDS: { value: BackendKind; label: string; blurb: string }[] = [
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
function cliPath(backend: BackendConfig, detected: DetectedClis | null): string | null {
  if (backend.cli.path.trim()) return backend.cli.path.trim();
  if (backend.kind === "claudeCode") return detected?.claudeCode ?? null;
  if (backend.kind === "codex") return detected?.codex ?? null;
  return null;
}

export default function AiReport() {
  const { goto, selection } = useApp();

  const [courseId, setCourseId] = useState(selection.courseId ?? "");
  const [tree, setTree] = useState<ReportTree | null>(null);
  const [loadingTree, setLoadingTree] = useState(false);
  const [picked, setPicked] = useState<Set<string>>(new Set());

  const [taskBook, setTaskBook] = useState("");
  const [requirements, setRequirements] = useState("");
  const [template, setTemplate] = useState("");

  const [backend, setBackend] = useState<BackendConfig>({
    kind: "api",
    api: { baseUrl: "https://api.deepseek.com", apiKey: "", model: "deepseek-chat" },
    cli: { path: "", model: "" },
  });
  const [remember, setRemember] = useState(false);
  const [detected, setDetected] = useState<DetectedClis | null>(null);

  const setApi = (patch: Partial<BackendConfig["api"]>) =>
    setBackend((prev) => ({ ...prev, api: { ...prev.api, ...patch } }));
  const setCli = (patch: Partial<BackendConfig["cli"]>) =>
    setBackend((prev) => ({ ...prev, cli: { ...prev.cli, ...patch } }));

  const [dest, setDest] = useState("");
  const [folder, setFolder] = useState("");

  const [result, setResult] = useState<ReportResult | null>(null);
  const [error, setError] = useState<api.ApiError | null>(null);
  const logRef = useRef<HTMLPreElement>(null);

  // 进度来自全局任务，切到别的页面再回来照样能看到。
  const tasks = useTasks();
  const task = tasks.running("report") ?? tasks.running("tree");
  const running = tasks.running("report") !== undefined;
  const log = task?.log ?? [];

  // 上次用过的接口设置（API Key 只在用户勾了「记住」时才回填）。
  useEffect(() => {
    void api
      .aiSettings()
      .then((s) => {
        setBackend(s.config);
        setRemember(s.rememberApiKey);
      })
      .catch(() => undefined);
    void api.detectCliBackends().then(setDetected).catch(() => undefined);
  }, []);

  useEffect(() => {
    logRef.current?.scrollTo({ top: logRef.current.scrollHeight });
  }, [log]);

  async function loadTree() {
    setLoadingTree(true);
    setError(null);
    setTree(null);
    try {
      const t = await tasks.run("tree", `读取课程结构 ${courseId.trim()}`, () =>
        api.reportTree(courseId.trim()),
      );
      setTree(t);
      // 默认全选：多数人是整门课程写报告，取消比逐个勾更省事。
      setPicked(allGameIds(t));
      if (!folder.trim()) setFolder(`${t.courseName}-实践报告`);
    } catch (e) {
      setError(api.toApiError(e));
    } finally {
      setLoadingTree(false);
    }
  }

  const selected = useMemo(
    () => (tree ? selectedHomeworks(tree, picked) : []),
    [tree, picked],
  );

  const totalPicked = selected.reduce((n, h) => n + h.challenges.length, 0);

  async function start() {
    setResult(null);
    setError(null);
    try {
      await api.saveAiSettings(backend, remember).catch(() => undefined);
      const r = await tasks.run(
        "report",
        `生成报告 · ${tree?.courseName ?? ""}`,
        () =>
          api.generateReport(
            {
              courseName: tree?.courseName ?? "",
              dest,
              folder: folder.trim() || null,
              homeworks: selected,
              taskBook,
              requirements,
              template,
            },
            backend,
          ),
        (r) => r.file,
      );
      setResult(r);
    } catch (e) {
      setError(api.toApiError(e));
    }
  }

  // 本地 CLI 用你已登录的账号，不需要 key；只有 API 后端才必须填。
  const backendReady =
    backend.kind === "api" ? backend.api.apiKey.trim() !== "" : cliPath(backend, detected) !== null;
  const canStart = !running && totalPicked > 0 && dest !== "" && backendReady;

  return (
    <div className="page">
      <header className="page-head">
        <h1>实验报告</h1>
        <p className="muted">
          勾选要写进报告的关卡，粘贴任务书与报告模板，AI 会按「课程任务概述 / 任务实施过程与分析 /
          课程总结」的结构逐节起草，并把每关的题目和你的代码另存为素材。
          需要截图和运行结果的地方会留占位符，<strong>成稿前请务必自己通读核对</strong>。
        </p>
      </header>

      <section className="card">
        <div className="card-head">
          <h2>1 · 选择关卡</h2>
          {tree ? <Badge tone="ok">{tree.courseName}</Badge> : null}
        </div>
        <div className="form-grid">
          <Field label="courseId" hint="可在「浏览」页点击课程的 id 徽章复制。">
            <div className="input-row">
              <input
                className="input mono"
                value={courseId}
                onChange={(e) => setCourseId(e.target.value)}
                placeholder="courseId"
                disabled={running || loadingTree}
                spellCheck={false}
              />
              <button
                className="btn"
                onClick={() => void loadTree()}
                disabled={running || loadingTree || courseId.trim() === ""}
              >
                {loadingTree ? <Spinner /> : <ListTree size={14} />} 读取课程结构
              </button>
            </div>
          </Field>
        </div>

        {loadingTree ? (
          <pre className="code-block log">{log.join("\n") || "…"}</pre>
        ) : tree ? (
          <ChallengeTree tree={tree} picked={picked} onChange={setPicked} disabled={running} />
        ) : (
          <Empty
            title="还没有读取课程结构"
            hint="填入 courseId 后点「读取课程结构」，会列出每个实训及其关卡。"
          />
        )}
      </section>

      <section className="card">
        <div className="card-head">
          <h2>2 · 粘贴材料</h2>
          <span className="muted small">都可留空，但给得越全，生成的报告越贴合要求</span>
        </div>
        <div className="form-grid">
          <Field label="课程任务书" hint="课程发的任务书全文，AI 据此理解每个实训的背景与要求。">
            <textarea
              className="input textarea"
              rows={6}
              value={taskBook}
              onChange={(e) => setTaskBook(e.target.value)}
              placeholder="粘贴任务书正文…"
              disabled={running}
            />
          </Field>
          <Field label="实验 / 报告要求" hint="对篇幅、重点关卡数量、必写小节等的额外要求。">
            <textarea
              className="input textarea"
              rows={5}
              value={requirements}
              onChange={(e) => setRequirements(e.target.value)}
              placeholder="粘贴实验要求…"
              disabled={running}
            />
          </Field>
          <Field
            label="报告模板"
            hint="粘贴学院给的报告模板。只用来确定章节结构，字体、行距、页码这类排版要求会被忽略。"
          >
            <textarea
              className="input textarea"
              rows={6}
              value={template}
              onChange={(e) => setTemplate(e.target.value)}
              placeholder="粘贴报告模板…"
              disabled={running}
            />
          </Field>
        </div>
      </section>

      <section className="card">
        <div className="card-head">
          <h2>3 · 由谁来写</h2>
        </div>

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
                    没在 PATH 上找到 <span className="mono">
                      {backend.kind === "claudeCode" ? "claude" : "codex"}
                    </span>
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
      </section>

      <section className="card">
        <div className="card-head">
          <h2>4 · 输出</h2>
        </div>
        <div className="form-grid">
          <Field label="保存到" hint="会在该目录下建一个子文件夹，放 报告.md、images/ 和 素材/。">
            <div className="input-row">
              <input
                className="input mono"
                value={dest}
                onChange={(e) => setDest(e.target.value)}
                placeholder="选择一个目录…"
                disabled={running}
                spellCheck={false}
              />
              <button
                className="btn"
                onClick={() =>
                  void open({ directory: true, multiple: false, title: "选择保存目录" }).then(
                    (p) => {
                      if (typeof p === "string") setDest(p);
                    },
                  )
                }
                disabled={running}
              >
                <Folder size={14} /> 浏览…
              </button>
            </div>
          </Field>
          <Field label="子文件夹名">
            <input
              className="input"
              value={folder}
              onChange={(e) => setFolder(e.target.value)}
              placeholder="默认取课程名"
              disabled={running}
            />
          </Field>
        </div>

        <div className="btn-row">
          <button className="btn btn-primary" onClick={() => void start()} disabled={!canStart}>
            {running ? <Spinner /> : <Sparkles size={14} />} 开始生成
          </button>
          {running ? (
            <button className="btn btn-danger" onClick={() => void api.cancelReport()}>
              <X size={14} /> 取消
            </button>
          ) : null}
          {!running && totalPicked > 0 ? (
            <span className="muted small">
              将调用 AI 约 {selected.length + 3} 次（{selected.length} 个小节 + 绪言 + 第 1、3 章）
            </span>
          ) : null}
        </div>
      </section>

      {running || (log.length > 0 && !loadingTree) ? (
        <section className="card">
          <div className="card-head">
            <h2>进度</h2>
            {running ? <Badge tone="warn">生成中</Badge> : <Badge tone="ok">已结束</Badge>}
          </div>
          <pre className="code-block log" ref={logRef}>
            {log.join("\n") || "…"}
          </pre>
        </section>
      ) : null}

      {result ? <ResultCard result={result} /> : null}
      {error ? <ErrorBox error={error} onFixCookies={() => goto("account")} /> : null}
    </div>
  );
}

function ResultCard({ result }: { result: ReportResult }) {
  return (
    <section className="card">
      <div className="card-head">
        <h2>生成完成</h2>
        <button className="btn btn-sm" onClick={() => void revealItemInDir(result.file)}>
          <FolderOpen size={13} /> 打开文件夹
        </button>
      </div>
      <p className="mono wrap">{result.file}</p>

      <ul className="plain-list">
        <li>
          正文 <Badge tone="ok">{result.sections} 个小节</Badge>
        </li>
        <li>
          素材 <Badge tone="ok">{result.materials} 个关卡</Badge>
        </li>
        <li>
          待你补充 <Badge tone={result.placeholders > 0 ? "warn" : "ok"}>
            {result.placeholders} 处截图 / 运行结果
          </Badge>
        </li>
        {result.failed.length > 0 ? (
          <li>
            生成失败 <Badge tone="error">{result.failed.join("、")}</Badge>
            <span className="muted small block">
              这些小节在正文里留了提示，素材已存好，可以手动补写或重新生成。
            </span>
          </li>
        ) : null}
      </ul>

      <p className="muted small">
        报告是 AI 起草的初稿：请逐节核对代码与描述是否与你实际提交的一致，补齐 🖼️ / 📋
        占位符，并填写封面的个人信息后再提交。
      </p>
    </section>
  );
}
