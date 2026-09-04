// 任务页：所有长任务的进度与完整日志，跑着的和跑完的都在。
// 页面切换不再丢进度——日志由 TasksProvider 在 app 层累积。
import { useEffect, useRef, useState } from "react";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { ChevronDown, ChevronRight, Eraser, FolderOpen, ListChecks, X } from "lucide-react";

import { elapsed, KIND_LABEL, useTasks, type Task } from "../tasks";
import { Badge, Empty, Spinner } from "../ui";

const TONE: Record<Task["status"], { tone: "neutral" | "ok" | "warn" | "error"; text: string }> = {
  running: { tone: "warn", text: "进行中" },
  done: { tone: "ok", text: "已完成" },
  error: { tone: "error", text: "失败" },
  cancelled: { tone: "neutral", text: "已取消" },
};

export default function Tasks() {
  const { tasks, runningCount, cancel, clearFinished } = useTasks();
  // 跑着的任务要让耗时读数动起来。
  const [, tick] = useState(0);
  useEffect(() => {
    if (runningCount === 0) return;
    const t = setInterval(() => tick((n) => n + 1), 1000);
    return () => clearInterval(t);
  }, [runningCount]);

  const finished = tasks.filter((t) => t.status !== "running").length;

  return (
    <div className="page">
      <header className="page-head">
        <h1>任务</h1>
        <p className="muted">
          导出与报告生成都在后台跑，切到别的页面不会中断，进度和日志会一直记在这里。
        </p>
      </header>

      {tasks.length === 0 ? (
        <Empty
          icon={<ListChecks size={30} strokeWidth={1.5} />}
          title="还没有任务"
          hint="在「导出」或「实验报告」页发起的任务会出现在这里。"
        />
      ) : (
        <>
          <div className="list-head">
            <span className="muted">
              共 {tasks.length} 个任务
              {runningCount > 0 ? `，${runningCount} 个进行中` : ""}
            </span>
            {finished > 0 ? (
              <button className="btn btn-ghost btn-sm" onClick={clearFinished}>
                <Eraser size={13} /> 清除已结束
              </button>
            ) : null}
          </div>

          <div className="task-list">
            {tasks.map((t) => (
              <TaskCard key={t.id} task={t} onCancel={() => cancel(t.id)} />
            ))}
          </div>
        </>
      )}
    </div>
  );
}

function TaskCard({ task, onCancel }: { task: Task; onCancel: () => void }) {
  // 进行中的默认展开——那才是用户切过来想看的东西。
  const [open, setOpen] = useState(task.status === "running");
  const logRef = useRef<HTMLPreElement>(null);
  const meta = TONE[task.status];

  useEffect(() => {
    if (open) logRef.current?.scrollTo({ top: logRef.current.scrollHeight });
  }, [task.log, open]);

  return (
    <section className="card task-card">
      <div className="task-head">
        <button
          type="button"
          className="tree-toggle"
          onClick={() => setOpen((v) => !v)}
          aria-label={open ? "收起日志" : "展开日志"}
        >
          {open ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
        </button>
        <div className="task-title">
          <span className="task-name">{task.title}</span>
          <span className="muted small">
            {KIND_LABEL[task.kind]} · {elapsed(task)}
            {task.log.length > 0 ? ` · ${task.log.length} 行日志` : ""}
          </span>
        </div>
        {task.status === "running" ? <Spinner /> : null}
        <Badge tone={meta.tone}>{meta.text}</Badge>
        {task.status === "running" ? (
          <button className="btn btn-danger btn-sm" onClick={onCancel}>
            <X size={13} /> 取消
          </button>
        ) : null}
        {task.resultDir ? (
          <button className="btn btn-sm" onClick={() => void revealItemInDir(task.resultDir!)}>
            <FolderOpen size={13} /> 打开文件夹
          </button>
        ) : null}
      </div>

      {task.error && task.status === "error" ? (
        <div className="errorbox-msg">{task.error.message}</div>
      ) : null}

      {open ? (
        <pre className="code-block log" ref={logRef}>
          {task.log.join("\n") || (task.status === "running" ? "…" : "（没有日志）")}
        </pre>
      ) : null}
    </section>
  );
}
