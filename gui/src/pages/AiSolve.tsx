// AI 做实验页：勾选课程里要做的关卡，AI 读取任务描述和仓库里已有的代码，
// 给出能通过测评、可以直接复制粘贴提交的完整代码文件。
import { useEffect, useMemo, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { AlertTriangle, Folder, FolderOpen, ListTree, Wand2, X } from "lucide-react";

import { AiBackendPicker, backendReady } from "../AiBackendPicker";
import * as api from "../api";
import { ChallengeTree, selectedHomeworks } from "../ChallengeTree";
import { useApp } from "../context";
import { useTasks } from "../tasks";
import type { BackendConfig, DetectedClis, ReportTree, SolveResult } from "../types";
import { Badge, Empty, ErrorBox, Field, Spinner } from "../ui";

export default function AiSolve() {
  const { goto, selection } = useApp();

  const [courseId, setCourseId] = useState(selection.courseId ?? "");
  const [tree, setTree] = useState<ReportTree | null>(null);
  const [loadingTree, setLoadingTree] = useState(false);
  const [picked, setPicked] = useState<Set<string>>(new Set());

  const [requirements, setRequirements] = useState("");

  const [backend, setBackend] = useState<BackendConfig>({
    kind: "api",
    api: { baseUrl: "https://api.deepseek.com", apiKey: "", model: "deepseek-chat" },
    cli: { path: "", model: "" },
  });
  const [remember, setRemember] = useState(false);
  const [detected, setDetected] = useState<DetectedClis | null>(null);

  const [dest, setDest] = useState("");
  const [folder, setFolder] = useState("");

  const [result, setResult] = useState<SolveResult | null>(null);
  const [error, setError] = useState<api.ApiError | null>(null);
  const logRef = useRef<HTMLPreElement>(null);

  // 进度来自全局任务，切到别的页面再回来照样能看到。
  const tasks = useTasks();
  const task = tasks.running("solve") ?? tasks.running("tree");
  const running = tasks.running("solve") !== undefined;
  const log = task?.log ?? [];

  // 复用「实验报告」页记住的同一套接口设置。
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
      if (!folder.trim()) setFolder(`${t.courseName}-AI答案`);
    } catch (e) {
      setError(api.toApiError(e));
    } finally {
      setLoadingTree(false);
    }
  }

  const selected = useMemo(() => (tree ? selectedHomeworks(tree, picked) : []), [tree, picked]);
  const totalPicked = selected.reduce((n, h) => n + h.challenges.length, 0);

  async function start() {
    setResult(null);
    setError(null);
    try {
      await api.saveAiSettings(backend, remember).catch(() => undefined);
      const r = await tasks.run(
        "solve",
        `AI 做实验 · ${tree?.courseName ?? ""}`,
        () =>
          api.solveSelection(
            {
              courseName: tree?.courseName ?? "",
              dest,
              folder: folder.trim() || null,
              homeworks: selected,
              requirements,
            },
            backend,
          ),
        (r) => r.summaryFile,
      );
      setResult(r);
    } catch (e) {
      setError(api.toApiError(e));
    }
  }

  const canStart = !running && totalPicked > 0 && dest !== "" && backendReady(backend, detected);

  return (
    <div className="page">
      <header className="page-head">
        <h1>AI 做实验</h1>
        <p className="muted">
          勾选要做的关卡，AI 会读取每一关的任务描述和仓库里已有的代码，逐关给出完整代码。
          除了按关卡分开保存的文件，根目录下还会有一份 <span className="mono">答案汇总.md</span>
          ，把所有关卡的代码按顺序整理成一个文件，方便直接复制粘贴。
          <strong>这是 AI 的初稿，提交前请自己通读核对</strong>。
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
          <h2>2 · 额外要求</h2>
          <span className="muted small">可留空</span>
        </div>
        <div className="form-grid">
          <Field
            label="额外要求"
            hint="例如指定语言版本、禁止用某些语法、代码风格要求等，会随每一关的材料一起发给 AI。"
          >
            <textarea
              className="input textarea"
              rows={4}
              value={requirements}
              onChange={(e) => setRequirements(e.target.value)}
              placeholder="留空即可…"
              disabled={running}
            />
          </Field>
        </div>
      </section>

      <section className="card">
        <div className="card-head">
          <h2>3 · 由谁来写</h2>
        </div>
        <AiBackendPicker
          backend={backend}
          setBackend={setBackend}
          remember={remember}
          setRemember={setRemember}
          running={running}
          detected={detected}
        />
      </section>

      <section className="card">
        <div className="card-head">
          <h2>4 · 输出</h2>
        </div>
        <div className="form-grid">
          <Field label="保存到" hint="会在该目录下建一个子文件夹，按「实训/关卡」两级存放题目和代码文件。">
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
            {running ? <Spinner /> : <Wand2 size={14} />} 开始生成
          </button>
          {running ? (
            <button className="btn btn-danger" onClick={() => void api.cancelSolve()}>
              <X size={14} /> 取消
            </button>
          ) : null}
          {!running && totalPicked > 0 ? (
            <span className="muted small">将调用 AI 约 {totalPicked} 次，每关一次</span>
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

function ResultCard({ result }: { result: SolveResult }) {
  return (
    <section className="card">
      <div className="card-head">
        <h2>生成完成</h2>
        <button className="btn btn-sm btn-primary" onClick={() => void revealItemInDir(result.summaryFile)}>
          <FolderOpen size={13} /> 打开答案汇总.md
        </button>
        <button className="btn btn-sm" onClick={() => void revealItemInDir(result.dir)}>
          <FolderOpen size={13} /> 打开文件夹
        </button>
      </div>
      <p className="mono wrap">{result.summaryFile}</p>

      <ul className="plain-list">
        <li>
          成功 <Badge tone="ok">{result.solved} 关</Badge>
          {result.failed > 0 ? (
            <>
              {" "}
              失败 <Badge tone="error">{result.failed} 关</Badge>
            </>
          ) : null}
        </li>
      </ul>

      <ul className="rows">
        {result.challenges.map((c, i) => (
          <li key={`${c.name}-${i}`} className="row">
            <div className="row-main">
              <div className="row-title">
                {c.name}
                {c.error ? <Badge tone="error">失败</Badge> : <Badge tone="ok">{c.files.length} 个文件</Badge>}
              </div>
              {c.error ? (
                <div className="row-meta muted small">{c.error}</div>
              ) : (
                <div className="row-meta mono small">{c.files.join("、")}</div>
              )}
            </div>
          </li>
        ))}
      </ul>

      <div className="notice notice-warn">
        <AlertTriangle size={14} />
        <span className="notice-body">
          AI 给出的代码不保证一定能通过测评：请对照任务描述逐关核对，确认理解代码内容后再复制提交到平台。
        </span>
      </div>
    </section>
  );
}
