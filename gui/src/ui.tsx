// Small presentational building blocks shared by every page.
import type { ReactNode } from "react";
import { AlertCircle, Check, Copy, Inbox, Loader2 } from "lucide-react";

import type { ApiError } from "./api";

export function Spinner({ label }: { label?: string }) {
  return (
    <span className="spinner-wrap">
      <Loader2 className="spin" size={14} aria-hidden="true" />
      {label ? <span className="muted">{label}</span> : null}
    </span>
  );
}

export function Loading({ label = "加载中…" }: { label?: string }) {
  return (
    <div className="state-block">
      <Spinner label={label} />
    </div>
  );
}

export function Empty({ icon, title, hint }: { icon?: ReactNode; title: string; hint?: ReactNode }) {
  return (
    <div className="state-block">
      <div className="empty-icon" aria-hidden="true">
        {icon ?? <Inbox size={30} strokeWidth={1.5} />}
      </div>
      <div className="empty-title">{title}</div>
      {hint ? <div className="muted empty-hint">{hint}</div> : null}
    </div>
  );
}

/** Renders an `ApiError`, offering the account page when the session expired. */
export function ErrorBox({ error, onFixCookies }: { error: ApiError; onFixCookies?: () => void }) {
  return (
    <div className="errorbox" role="alert">
      <div className="errorbox-head">
        <AlertCircle size={16} aria-hidden="true" />
        <strong>出错了</strong>
        {error.status ? <span className="badge badge-error">HTTP {error.status}</span> : null}
      </div>
      <div className="errorbox-msg">{error.message}</div>
      {error.needsCookies && onFixCookies ? (
        <button className="btn btn-primary btn-sm" onClick={onFixCookies}>
          去登录
        </button>
      ) : null}
      {error.data !== undefined && error.data !== null ? (
        <details>
          <summary>服务器响应</summary>
          <pre className="code-block">{JSON.stringify(error.data, null, 2)}</pre>
        </details>
      ) : null}
    </div>
  );
}

export function Field({
  label,
  hint,
  children,
}: {
  label: string;
  hint?: ReactNode;
  children: ReactNode;
}) {
  return (
    <label className="field">
      <span className="field-label">{label}</span>
      {children}
      {hint ? <span className="field-hint">{hint}</span> : null}
    </label>
  );
}

export function Badge({
  children,
  tone = "neutral",
}: {
  children: ReactNode;
  tone?: "neutral" | "ok" | "warn" | "error";
}) {
  return <span className={`badge badge-${tone}`}>{children}</span>;
}

/** A copy-to-clipboard identifier chip — ids are what every CLI command needs. */
export function IdChip({ label, value }: { label: string; value?: string | number | null }) {
  if (value === undefined || value === null || value === "") return null;
  const text = String(value);
  return (
    <button
      type="button"
      className="idchip"
      title={`点击复制 ${text}`}
      onClick={(e) => {
        e.stopPropagation();
        void navigator.clipboard.writeText(text);
      }}
    >
      <span className="idchip-label">{label}</span>
      <span className="idchip-value">{text}</span>
      <Copy size={11} aria-hidden="true" />
    </button>
  );
}

export function Json({ value }: { value: unknown }) {
  return <pre className="code-block">{JSON.stringify(value, null, 2)}</pre>;
}

export function Breadcrumbs({ items }: { items: { label: string; onClick?: () => void }[] }) {
  return (
    <nav className="breadcrumbs" aria-label="面包屑导航">
      {items.map((item, i) => (
        <span key={i} className="breadcrumb">
          {i > 0 ? <span className="breadcrumb-sep">›</span> : null}
          {item.onClick ? (
            <button className="linkbtn" onClick={item.onClick}>
              {item.label}
            </button>
          ) : (
            <span className="breadcrumb-current">{item.label}</span>
          )}
        </span>
      ))}
    </nav>
  );
}

/** Transient "did it" confirmation line, e.g. after saving a file. */
export function Notice({ children }: { children: ReactNode }) {
  return (
    <div className="notice">
      <span className="notice-body">
        <Check size={16} aria-hidden="true" />
        {children}
      </span>
    </div>
  );
}
