import { useCallback, useEffect, useMemo, useState } from "react";
import type { LucideIcon } from "lucide-react";
import {
  CircleUser,
  Download,
  FolderTree,
  GraduationCap,
  ListChecks,
  ScrollText,
  Sparkles,
  TerminalSquare,
  Wand2,
} from "lucide-react";

import * as api from "./api";
import { AppContext, type Page, type Selection } from "./context";
import Account from "./pages/Account";
import ApiConsole from "./pages/ApiConsole";
import Browse from "./pages/Browse";
import { TasksProvider, useTasks } from "./tasks";
import AiReport from "./pages/AiReport";
import AiSolve from "./pages/AiSolve";
import Export from "./pages/Export";
import Report from "./pages/Report";
import Tasks from "./pages/Tasks";
import type { CookieStatus, UserInfo } from "./types";

const NAV: { page: Page; label: string; Icon: LucideIcon; hint: string }[] = [
  { page: "browse", label: "浏览", Icon: FolderTree, hint: "课程 → 作业 → 关卡 → 任务与代码" },
  { page: "export", label: "导出", Icon: Download, hint: "把任务描述和代码存到本地" },
  { page: "report", label: "分数", Icon: ScrollText, hint: "作业报告：每关分数与改动文件" },
  { page: "aireport", label: "实验报告", Icon: Sparkles, hint: "用 AI 起草整门课程的实践报告" },
  { page: "aisolve", label: "AI 做实验", Icon: Wand2, hint: "用 AI 给出关卡的完整代码，供复制提交" },
  { page: "tasks", label: "任务", Icon: ListChecks, hint: "进行中与已完成任务的进度和日志" },
  { page: "api", label: "API", Icon: TerminalSquare, hint: "对任意接口发起签名请求" },
  { page: "account", label: "账号", Icon: CircleUser, hint: "登录状态与 Cookie" },
];

export default function App() {
  return (
    <TasksProvider>
      <AppShell />
    </TasksProvider>
  );
}

function AppShell() {
  const [status, setStatus] = useState<CookieStatus | null>(null);
  const [user, setUser] = useState<UserInfo | null>(null);
  const [page, setPage] = useState<Page>("browse");
  const [selection, setSelection] = useState<Selection>({});
  const { runningCount } = useTasks();

  const refreshSession = useCallback(async () => {
    const s = await api.cookieStatus().catch(() => null);
    setStatus(s);
    setUser(s?.loaded ? await api.me().catch(() => null) : null);
    return s;
  }, []);

  // On first paint the backend may already have auto-loaded a cookies.txt;
  // if it hasn't, there is nothing to browse yet — start on the account page.
  useEffect(() => {
    void refreshSession().then((s) => {
      if (!s?.loaded) setPage("account");
    });
  }, [refreshSession]);

  const goto = useCallback((next: Page, sel?: Selection) => {
    if (sel) setSelection((prev) => ({ ...prev, ...sel }));
    setPage(next);
  }, []);

  const select = useCallback((sel: Selection) => {
    setSelection((prev) => ({ ...prev, ...sel }));
  }, []);

  const ctx = useMemo(
    () => ({ status, user, refreshSession, page, goto, selection, select }),
    [status, user, refreshSession, page, goto, selection, select],
  );

  const displayName = user?.real_name || user?.username || user?.login;

  return (
    <AppContext.Provider value={ctx}>
      <div className="app">
        <aside className="sidebar">
          <div className="brand">
            <div className="brand-mark" aria-hidden="true">
              <GraduationCap size={20} />
            </div>
            <div className="brand-text">
              <div className="brand-name">EduCoder Helper</div>
              <div className="brand-sub">实验内容查询与导出</div>
            </div>
          </div>

          <nav className="nav">
            {NAV.map((item) => (
              <button
                key={item.page}
                className={`nav-item${page === item.page ? " is-active" : ""}`}
                onClick={() => setPage(item.page)}
                title={item.hint}
              >
                <item.Icon className="nav-icon" size={17} aria-hidden="true" />
                <span className="nav-label">{item.label}</span>
                {item.page === "tasks" && runningCount > 0 ? (
                  <span className="nav-count">{runningCount}</span>
                ) : null}
              </button>
            ))}
          </nav>

          <button className="session" onClick={() => setPage("account")} title="账号与登录状态">
            <span className={`dot ${status?.loaded ? "dot-ok" : "dot-off"}`} aria-hidden="true" />
            <span className="session-text">
              {status?.loaded ? (
                <>
                  <span className="session-name">{displayName ?? "已登录"}</span>
                  <span className="session-sub">{user?.login ? `@${user.login}` : status.host}</span>
                </>
              ) : (
                <>
                  <span className="session-name">未登录</span>
                  <span className="session-sub">点击登录</span>
                </>
              )}
            </span>
          </button>
        </aside>

        <main className="content">
          {page === "browse" ? <Browse /> : null}
          {page === "export" ? <Export /> : null}
          {page === "report" ? <Report /> : null}
          {page === "aireport" ? <AiReport /> : null}
          {page === "aisolve" ? <AiSolve /> : null}
          {page === "tasks" ? <Tasks /> : null}
          {page === "api" ? <ApiConsole /> : null}
          {page === "account" ? <Account /> : null}
        </main>
      </div>
    </AppContext.Provider>
  );
}
