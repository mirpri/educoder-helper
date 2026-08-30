import { useCallback, useEffect, useMemo, useState } from "react";
import type { LucideIcon } from "lucide-react";
import {
  CircleUser,
  Download,
  FolderTree,
  GraduationCap,
  ScrollText,
  TerminalSquare,
} from "lucide-react";

import * as api from "./api";
import { AppContext, type Page, type Selection } from "./context";
import Account from "./pages/Account";
import ApiConsole from "./pages/ApiConsole";
import Browse from "./pages/Browse";
import Export from "./pages/Export";
import Report from "./pages/Report";
import type { CookieStatus, UserInfo } from "./types";

const NAV: { page: Page; label: string; Icon: LucideIcon; hint: string }[] = [
  { page: "browse", label: "浏览", Icon: FolderTree, hint: "课程 → 作业 → 关卡 → 任务与代码" },
  { page: "export", label: "导出", Icon: Download, hint: "把任务描述和代码存到本地" },
  { page: "report", label: "报告", Icon: ScrollText, hint: "作业报告：每关分数与改动文件" },
  { page: "api", label: "API", Icon: TerminalSquare, hint: "对任意接口发起签名请求" },
  { page: "account", label: "账号", Icon: CircleUser, hint: "登录状态与 Cookie" },
];

export default function App() {
  const [status, setStatus] = useState<CookieStatus | null>(null);
  const [user, setUser] = useState<UserInfo | null>(null);
  const [page, setPage] = useState<Page>("browse");
  const [selection, setSelection] = useState<Selection>({});

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
          {page === "api" ? <ApiConsole /> : null}
          {page === "account" ? <Account /> : null}
        </main>
      </div>
    </AppContext.Provider>
  );
}
