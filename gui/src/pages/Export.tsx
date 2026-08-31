// 导出页：对应 `edu export challenge|shixun|course`，带目标目录选择、
// 实时日志、取消，以及完成后打开文件夹。
import { useEffect, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { Download, Folder, FolderOpen, X } from "lucide-react";

import * as api from "../api";
import { useApp } from "../context";
import type { ExportResult, ImageMode } from "../types";
import { Badge, ErrorBox, Field, Spinner } from "../ui";

type Level = "challenge" | "shixun" | "course";

// 任务描述里的图片是站内相对路径（/api/attachments/…），本地打开必然裂图。
// 这些图片无需登录即可访问，所以「改写链接」和「下载到本地」都成立。
const IMAGE_MODES: { value: ImageMode; label: string; blurb: string }[] = [
  {
    value: "link",
    label: "改写链接",
    blurb: "把图片地址改成 educoder.net 的完整链接。文件小，联网时才能看到图。",
  },
  {
    value: "download",
    label: "保存图片",
    blurb: "把图片下载到每关的 images/ 子目录并改为相对路径。完全离线可读，导出更慢。",
  },
  {
    value: "keep",
    label: "保持原样",
    blurb: "不改动任务描述。图片在本地打不开，适合只想要原始文本的场景。",
  },
];

const LEVELS: { value: Level; label: string; idLabel: string; blurb: string }[] = [
  {
    value: "challenge",
    label: "单个关卡",
    idLabel: "gameId",
    blurb: "导出一关：README.md（任务描述）+ 该关的可编辑文件。",
  },
  {
    value: "shixun",
    label: "整个实训",
    idLabel: "myshixunId",
    blurb: "导出一个实训的所有关卡，每关一个 NN_名称 子目录。",
  },
  {
    value: "course",
    label: "整门课程",
    idLabel: "courseId",
    blurb: "导出课程下所有实训作业，每个作业一个子目录。没有实例的实训可先自动进入。",
  },
];

export default function Export() {
  const { goto, selection } = useApp();
  const [level, setLevel] = useState<Level>(selection.exportLevel ?? "shixun");
  const [id, setId] = useState("");
  const [dest, setDest] = useState("");
  const [name, setName] = useState("");
  const [enterIfNeeded, setEnterIfNeeded] = useState(true);
  const [images, setImages] = useState<ImageMode>("link");
  const [running, setRunning] = useState(false);
  const [log, setLog] = useState<string[]>([]);
  const [result, setResult] = useState<ExportResult | null>(null);
  const [error, setError] = useState<api.ApiError | null>(null);
  const logRef = useRef<HTMLPreElement>(null);

  const meta = LEVELS.find((l) => l.value === level)!;

  // Prefill the id (and a sensible folder name) from whatever the user was
  // looking at on the 浏览 page.
  useEffect(() => {
    const defaults: Record<Level, [string | undefined, string | undefined]> = {
      challenge: [selection.gameId, undefined],
      shixun: [selection.myshixunId, selection.homeworkName],
      course: [selection.courseId, selection.courseName],
    };
    const [nextId, nextName] = defaults[level];
    setId(nextId ?? "");
    setName(nextName ?? "");
  }, [level, selection]);

  // Stream the backend's progress lines into the log pane.
  useEffect(() => {
    const unlisten = api.onExportLog((line) => setLog((prev) => [...prev, line]));
    return () => {
      void unlisten.then((fn) => fn());
    };
  }, []);

  useEffect(() => {
    logRef.current?.scrollTo({ top: logRef.current.scrollHeight });
  }, [log]);

  async function pickDest() {
    const picked = await open({ directory: true, multiple: false, title: "选择导出目录" });
    if (typeof picked === "string") setDest(picked);
  }

  async function start() {
    setRunning(true);
    setLog([]);
    setResult(null);
    setError(null);
    try {
      const folder = name.trim() || undefined;
      const r =
        level === "challenge"
          ? await api.exportChallenge(id.trim(), dest, folder, images)
          : level === "shixun"
            ? await api.exportShixun(id.trim(), dest, folder, images)
            : await api.exportCourse(id.trim(), dest, folder, enterIfNeeded, images);
      setResult(r);
    } catch (e) {
      setError(api.toApiError(e));
    } finally {
      setRunning(false);
    }
  }

  const canStart = !running && id.trim() !== "" && dest !== "";

  return (
    <div className="page">
      <header className="page-head">
        <h1>导出</h1>
        <p className="muted">
          把任务描述（README.md）和仓库里的可编辑文件按原目录结构存到本地。导出读取的是你自己实例的
          当前代码，不会提交或修改服务器上的内容。
        </p>
      </header>

      <section className="card">
        <div className="segmented">
          {LEVELS.map((l) => (
            <button
              key={l.value}
              className={`segment${level === l.value ? " is-active" : ""}`}
              onClick={() => setLevel(l.value)}
              disabled={running}
            >
              {l.label}
            </button>
          ))}
        </div>
        <p className="muted small">{meta.blurb}</p>

        <div className="form-grid">
          <Field label={meta.idLabel} hint="可在「浏览」页点击 id 徽章复制，或直接从这里输入。">
            <input
              className="input mono"
              value={id}
              onChange={(e) => setId(e.target.value)}
              placeholder={meta.idLabel}
              disabled={running}
              spellCheck={false}
            />
          </Field>

          <Field label="导出到" hint="导出内容会放进这个目录下的一个子文件夹。">
            <div className="input-row">
              <input
                className="input mono"
                value={dest}
                onChange={(e) => setDest(e.target.value)}
                placeholder="选择一个目录…"
                disabled={running}
                spellCheck={false}
              />
              <button className="btn" onClick={() => void pickDest()} disabled={running}>
                <Folder size={14} /> 浏览…
              </button>
            </div>
          </Field>

          <Field label="子文件夹名（可选）" hint="留空则使用关卡 / 实训 / 课程的名称。">
            <input
              className="input"
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder="默认取导出对象的名称"
              disabled={running}
            />
          </Field>

          <Field
            label="任务描述里的图片"
            hint="任务描述用的是站内相对路径，不处理的话本地打开是裂图。"
          >
            <div className="segmented">
              {IMAGE_MODES.map((m) => (
                <button
                  key={m.value}
                  className={`segment${images === m.value ? " is-active" : ""}`}
                  onClick={() => setImages(m.value)}
                  disabled={running}
                  type="button"
                >
                  {m.label}
                </button>
              ))}
            </div>
            <p className="muted small">
              {IMAGE_MODES.find((m) => m.value === images)!.blurb}
            </p>
          </Field>

          {level === "course" ? (
            <label className="check">
              <input
                type="checkbox"
                checked={enterIfNeeded}
                onChange={(e) => setEnterIfNeeded(e.target.checked)}
                disabled={running}
              />
              <span>
                未进入的实训自动进入
                <span className="muted small block">
                  关闭则跳过这些实训。开启会在服务器上创建新实例。
                </span>
              </span>
            </label>
          ) : null}
        </div>

        <div className="btn-row">
          <button className="btn btn-primary" onClick={() => void start()} disabled={!canStart}>
            {running ? <Spinner /> : <Download size={14} />} 开始导出
          </button>
          {running ? (
            <button className="btn btn-danger" onClick={() => void api.cancelExport()}>
              <X size={14} /> 取消
            </button>
          ) : null}
        </div>
      </section>

      {log.length > 0 || running ? (
        <section className="card">
          <div className="card-head">
            <h2>进度</h2>
            {running ? <Badge tone="warn">进行中</Badge> : <Badge tone="ok">已结束</Badge>}
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

function ResultCard({ result }: { result: ExportResult }) {
  const challenges = "challenges" in result ? result.challenges : null;
  const summary = "summary" in result ? result.summary : null;
  const files = "files" in result ? result.files : null;
  const imageCount = "images" in result ? result.images.length : 0;

  return (
    <section className="card">
      <div className="card-head">
        <h2>导出完成</h2>
        <button className="btn btn-sm" onClick={() => void revealItemInDir(result.dir)}>
          <FolderOpen size={13} /> 打开文件夹
        </button>
      </div>
      <p className="mono wrap">{result.dir}</p>

      {files ? (
        <ul className="plain-list mono">
          {files.map((f) => (
            <li key={f}>✓ {f}</li>
          ))}
          {imageCount > 0 ? <li>✓ images/（{imageCount} 张图片）</li> : null}
        </ul>
      ) : null}

      {challenges ? (
        <ul className="plain-list">
          {challenges.map((c, i) => (
            <li key={i}>
              <span className="mono">{String(c.position ?? "??").padStart(2, "0")}</span> {c.name}
              {c.error ? <Badge tone="error">{c.error}</Badge> : <Badge tone="ok">{c.files?.length ?? 0} 个文件</Badge>}
            </li>
          ))}
        </ul>
      ) : null}

      {summary ? (
        <ul className="plain-list">
          {summary.map((s, i) => (
            <li key={i}>
              {s.name}
              {s.error ? (
                <Badge tone="error">失败：{s.error}</Badge>
              ) : s.skipped ? (
                <Badge tone="warn">跳过：{s.skipped}</Badge>
              ) : (
                <Badge tone="ok">{s.challenges} 关</Badge>
              )}
            </li>
          ))}
        </ul>
      ) : null}
    </section>
  );
}
