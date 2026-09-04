// 长任务（导出 / 生成报告）的全局状态。
//
// 之前进度日志存在页面的局部 state 里，切到别的页面组件一卸载就没了——任务还在
// 后台跑，但用户看不见也取消不了。这里把任务提到 app 层：事件只在顶层订阅一次，
// 日志累积在 provider 里，页面只是它的一个视图，「任务」页是另一个。
import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState } from "react";
import type { ReactNode } from "react";

import * as api from "./api";

export type TaskKind = "export" | "report" | "tree" | "solve";
export type TaskStatus = "running" | "done" | "error" | "cancelled";

/** 后端推进度用的事件通道。取消也按通道分派。 */
export type TaskChannel = "export" | "report" | "solve";

export interface Task {
  id: string;
  kind: TaskKind;
  channel: TaskChannel;
  title: string;
  status: TaskStatus;
  startedAt: number;
  endedAt?: number;
  log: string[];
  error?: api.ApiError;
  /** 产物路径，任务页据此提供「打开文件夹」——中途切页面也不会丢。 */
  resultDir?: string;
}

const CHANNEL: Record<TaskKind, TaskChannel> = {
  export: "export",
  report: "report",
  tree: "report",
  solve: "solve",
};

export const KIND_LABEL: Record<TaskKind, string> = {
  export: "导出",
  report: "生成报告",
  tree: "读取课程结构",
  solve: "AI 做实验",
};

interface TasksValue {
  tasks: Task[];
  /** 正在跑的任务数，用来给侧栏加角标。 */
  runningCount: number;
  /** 某一类当前正在跑的任务，页面据此显示自己的进度。 */
  running: (kind: TaskKind) => Task | undefined;
  byId: (id: string) => Task | undefined;
  /**
   * 包一个异步调用成任务：日志会自动归到它名下。
   * `dirOf` 从结果里取出产物路径，供任务页直接打开。
   */
  run: <T>(
    kind: TaskKind,
    title: string,
    fn: () => Promise<T>,
    dirOf?: (result: T) => string | undefined,
  ) => Promise<T>;
  cancel: (id: string) => void;
  clearFinished: () => void;
}

const TasksContext = createContext<TasksValue | null>(null);

export function useTasks(): TasksValue {
  const ctx = useContext(TasksContext);
  if (!ctx) throw new Error("useTasks must be used inside <TasksProvider>");
  return ctx;
}

let seq = 0;
const nextId = () => `t${++seq}`;

export function TasksProvider({ children }: { children: ReactNode }) {
  const [tasks, setTasks] = useState<Task[]>([]);
  // 事件回调在订阅时就被捕获，拿不到最新的 tasks；用 ref 存"当前每个通道在跑
  // 的任务 id"，避免为了这个反复重订阅事件。
  const activeRef = useRef<Record<TaskChannel, string | null>>({
    export: null,
    report: null,
    solve: null,
  });

  const append = useCallback((channel: TaskChannel, line: string) => {
    const id = activeRef.current[channel];
    if (!id) return; // 没有归属的任务，多半是上一个任务收尾时的余音
    setTasks((prev) => prev.map((t) => (t.id === id ? { ...t, log: [...t.log, line] } : t)));
  }, []);

  // 只在这里订阅一次，页面来去都不影响日志累积。
  useEffect(() => {
    const un = [
      api.onExportLog((line) => append("export", line)),
      api.onReportLog((line) => append("report", line)),
      api.onSolveLog((line) => append("solve", line)),
    ];
    return () => {
      for (const u of un) void u.then((fn) => fn());
    };
  }, [append]);

  const run = useCallback(async <T,>(
    kind: TaskKind,
    title: string,
    fn: () => Promise<T>,
    dirOf?: (result: T) => string | undefined,
  ) => {
    const id = nextId();
    const channel = CHANNEL[kind];
    setTasks((prev) => [
      { id, kind, channel, title, status: "running", startedAt: Date.now(), log: [] },
      ...prev,
    ]);
    activeRef.current[channel] = id;

    const finish = (patch: Partial<Task>) => {
      if (activeRef.current[channel] === id) activeRef.current[channel] = null;
      setTasks((prev) =>
        prev.map((t) => (t.id === id ? { ...t, endedAt: Date.now(), ...patch } : t)),
      );
    };

    try {
      const result = await fn();
      finish({ status: "done", resultDir: dirOf?.(result) });
      return result;
    } catch (e) {
      const error = api.toApiError(e);
      // 后端把取消也报成错误，但它不是失败。
      finish({ status: error.message === "已取消" ? "cancelled" : "error", error });
      throw e;
    }
  }, []);

  const cancel = useCallback(
    (id: string) => {
      const task = tasks.find((t) => t.id === id);
      if (!task || task.status !== "running") return;
      if (task.channel === "export") void api.cancelExport();
      else if (task.channel === "report") void api.cancelReport();
      else void api.cancelSolve();
    },
    [tasks],
  );

  const value = useMemo<TasksValue>(
    () => ({
      tasks,
      runningCount: tasks.filter((t) => t.status === "running").length,
      running: (kind) => tasks.find((t) => t.kind === kind && t.status === "running"),
      byId: (id) => tasks.find((t) => t.id === id),
      run,
      cancel,
      clearFinished: () => setTasks((prev) => prev.filter((t) => t.status === "running")),
    }),
    [tasks, run, cancel],
  );

  return <TasksContext.Provider value={value}>{children}</TasksContext.Provider>;
}

/** 任务耗时，跑着的按现在算。 */
export function elapsed(task: Task, now = Date.now()): string {
  const ms = (task.endedAt ?? now) - task.startedAt;
  const s = Math.max(0, Math.round(ms / 1000));
  if (s < 60) return `${s} 秒`;
  const m = Math.floor(s / 60);
  return `${m} 分 ${s % 60} 秒`;
}
