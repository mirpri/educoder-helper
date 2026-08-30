// 账号页：浏览器登录（推荐）、cookies.txt 导入、直接粘贴，以及 `edu me` 的结果。
import { useEffect, useState } from "react";
import { open, save } from "@tauri-apps/plugin-dialog";
import {
  Braces,
  ClipboardPaste,
  FileText,
  LogIn,
  RefreshCw,
  Save,
  Trash2,
} from "lucide-react";

import * as api from "../api";
import { useApp } from "../context";
import { Badge, ErrorBox, Field, Json, Notice, Spinner } from "../ui";

const COOKIE_EXT = { name: "cookies.txt", extensions: ["txt"] };

export default function Account() {
  const { status, user, refreshSession } = useApp();
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<api.ApiError | null>(null);
  const [note, setNote] = useState<string | null>(null);
  const [paste, setPaste] = useState("");
  const [showRaw, setShowRaw] = useState(false);
  const [waitingLogin, setWaitingLogin] = useState(false);

  // The login window reports back through events, not a command result: the
  // user may take a while, or close the window without signing in.
  useEffect(() => {
    const unlisten = api.onLogin((e) => {
      setWaitingLogin(false);
      if (e.kind === "success") {
        setError(null);
        setNote("登录成功，已自动保存凭证");
        void refreshSession();
      } else if (e.kind === "timeout") {
        setError(api.toApiError("登录等待超时，请重试"));
      } else if (e.kind === "error") {
        setError(e.error);
      }
      // "closed" — the user gave up; leave the page as it was.
    });
    return () => {
      void unlisten.then((off) => off());
    };
  }, [refreshSession]);

  // Every mutation goes through here so busy/error/note stay consistent.
  async function run(what: string, fn: () => Promise<string | null>) {
    setBusy(what);
    setError(null);
    setNote(null);
    try {
      const message = await fn();
      if (message) setNote(message);
      await refreshSession();
    } catch (e) {
      setError(api.toApiError(e));
    } finally {
      setBusy(null);
    }
  }

  async function login() {
    setError(null);
    setNote(null);
    try {
      await api.openLoginWindow();
      setWaitingLogin(true);
    } catch (e) {
      setError(api.toApiError(e));
    }
  }

  async function pickFile() {
    const picked = await open({ multiple: false, filters: [COOKIE_EXT] });
    if (typeof picked !== "string") return;
    await run("file", async () => {
      await api.loadCookiesFile(picked);
      return `已载入 ${picked}`;
    });
  }

  async function usePaste() {
    if (!paste.trim()) return;
    await run("paste", async () => {
      const s = await api.loadCookiesText(paste);
      setPaste("");
      return s.source ? `已保存到 ${s.source}` : "已载入粘贴的 Cookie";
    });
  }

  async function saveCopy() {
    const target = await save({ defaultPath: "cookies.txt", filters: [COOKIE_EXT] });
    if (!target) return;
    await run("save", async () => `已写出 ${await api.exportCookiesFile(target)}`);
  }

  return (
    <div className="page">
      <header className="page-head">
        <h1>账号</h1>
        <p className="muted">
          本工具用你浏览器里的 educoder.net 登录凭证访问接口，不会保存密码。凭证会定期过期，
          若接口开始报 401 或返回 HTML，重新登录一次即可。
        </p>
      </header>

      <section className="card">
        <div className="card-head">
          <h2>当前状态</h2>
          {status?.loaded ? <Badge tone="ok">已登录</Badge> : <Badge tone="warn">未登录</Badge>}
        </div>

        {status?.loaded ? (
          <>
            <dl className="kv">
              {user?.real_name || user?.username ? (
                <>
                  <dt>姓名</dt>
                  <dd>
                    {user.real_name || user.username}
                    {user.real_name && user.username ? `（${user.username}）` : ""}
                  </dd>
                </>
              ) : null}
              {user?.login ? (
                <>
                  <dt>登录名</dt>
                  <dd>{user.login}</dd>
                </>
              ) : null}
              {user?.student_id ? (
                <>
                  <dt>学号</dt>
                  <dd>{user.student_id}</dd>
                </>
              ) : null}
              {user?.user_identity ? (
                <>
                  <dt>身份</dt>
                  <dd>{user.user_identity}</dd>
                </>
              ) : null}
              <dt>接口主机</dt>
              <dd>{status.host}</dd>
              <dt>凭证文件</dt>
              <dd className="mono wrap">{status.source ?? "（内存中）"}</dd>
              <dt>已解析</dt>
              <dd className="mono wrap">{status.names.join("、")}</dd>
            </dl>

            <div className="btn-row">
              <button
                className="btn"
                onClick={() => void run("refresh", async () => "已刷新")}
                disabled={!!busy}
              >
                {busy === "refresh" ? <Spinner /> : <RefreshCw size={14} />} 重新检测
              </button>
              <button className="btn" onClick={() => void login()} disabled={!!busy}>
                <LogIn size={14} /> 重新登录
              </button>
              <button className="btn" onClick={() => void saveCopy()} disabled={!!busy}>
                <Save size={14} /> 另存为 cookies.txt
              </button>
              <button
                className="btn btn-danger"
                onClick={() => void run("clear", async () => "已退出登录")}
                disabled={!!busy}
              >
                <Trash2 size={14} /> 退出登录
              </button>
              <button className="btn btn-ghost" onClick={() => setShowRaw((v) => !v)}>
                <Braces size={14} /> {showRaw ? "隐藏" : "查看"}原始 JSON
              </button>
            </div>
            {showRaw && user ? <Json value={user} /> : null}
          </>
        ) : (
          <p className="muted">还没有可用的登录凭证，请用下面任一方式登录。</p>
        )}
      </section>

      <section className="card card-hero">
        <div className="card-head">
          <h2>方式一 · 浏览器登录（推荐）</h2>
          <Badge tone="ok">最省事</Badge>
        </div>
        <p className="muted">
          点击后会打开一个 educoder.net 的登录窗口。像平常一样登录（账号密码、验证码、第三方登录都可以），
          成功后本应用会自动读取该窗口的登录凭证并关闭它 —— 不需要装扩展，也不用复制粘贴任何东西。
        </p>
        <div className="btn-row">
          <button className="btn btn-primary btn-lg" onClick={() => void login()} disabled={!!busy}>
            {waitingLogin ? <Spinner /> : <LogIn size={16} />}
            {waitingLogin ? "等待登录完成…" : "打开登录窗口"}
          </button>
          {waitingLogin ? (
            <button
              className="btn"
              onClick={() => {
                void api.closeLoginWindow();
                setWaitingLogin(false);
              }}
            >
              取消
            </button>
          ) : null}
        </div>
        <p className="muted small">
          凭证会以 cookies.txt 格式写入本应用的配置目录，命令行工具可通过{" "}
          <span className="mono">$EDUCODER_COOKIES</span> 指向同一个文件。
        </p>
      </section>

      <details className="card card-collapsible">
        <summary>
          <span className="card-summary-title">其他登录方式</span>
          <span className="muted small">已经有 cookies.txt，或想手动粘贴</span>
        </summary>

        <div className="card-body">
          <h3>
            <FileText size={15} /> 方式二 · 选择 cookies.txt
          </h3>
          <p className="muted">
            用浏览器扩展 <span className="mono">Get cookies.txt LOCALLY</span> 在已登录的 educoder.net
            页面导出。选定后路径会被记住，下次启动自动载入。
          </p>
          <div className="btn-row">
            <button className="btn" onClick={() => void pickFile()} disabled={!!busy}>
              {busy === "file" ? <Spinner /> : <FileText size={14} />} 选择文件…
            </button>
          </div>

          <h3>
            <ClipboardPaste size={15} /> 方式三 · 直接粘贴
          </h3>
          <Field
            label="Cookie 内容"
            hint="支持 Netscape cookies.txt 全文，也支持从开发者工具复制的 Cookie 请求头（a=1; b=2）。必须包含 _educoder_session。"
          >
            <textarea
              className="input textarea mono"
              rows={4}
              placeholder="_educoder_session=…; autologin_trustie=…"
              value={paste}
              onChange={(e) => setPaste(e.target.value)}
              spellCheck={false}
            />
          </Field>
          <div className="btn-row">
            <button className="btn" onClick={() => void usePaste()} disabled={!!busy || !paste.trim()}>
              {busy === "paste" ? <Spinner /> : <ClipboardPaste size={14} />} 使用这段 Cookie
            </button>
          </div>
        </div>
      </details>

      {note ? <Notice>{note}</Notice> : null}
      {error ? <ErrorBox error={error} /> : null}
    </div>
  );
}
