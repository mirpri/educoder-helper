// App-wide state: the cookie/session status, which page is showing, and the
// identifiers the user has drilled into. Pages hand ids to each other through
// `selection` so "导出这个实训" never means retyping an id.
import { createContext, useContext } from "react";

import type { CookieStatus, UserInfo } from "./types";

export type Page = "account" | "browse" | "export" | "report" | "aireport" | "tasks" | "api";

export interface Selection {
  courseId?: string;
  courseName?: string;
  homeworkName?: string;
  shixunId?: string;
  myshixunId?: string;
  gameId?: string;
  reportId?: string;
  /** Which export level the 导出 page should preselect. */
  exportLevel?: "challenge" | "shixun" | "course" | "selection";
}

export interface AppContextValue {
  status: CookieStatus | null;
  user: UserInfo | null;
  /** Re-reads the cookie status and current user; resolves to the new status. */
  refreshSession: () => Promise<CookieStatus | null>;
  page: Page;
  /** Navigate, optionally carrying identifiers to the target page. */
  goto: (page: Page, selection?: Selection) => void;
  selection: Selection;
  select: (selection: Selection) => void;
}

export const AppContext = createContext<AppContextValue | null>(null);

export function useApp(): AppContextValue {
  const ctx = useContext(AppContext);
  if (!ctx) throw new Error("useApp must be used inside <AppContext.Provider>");
  return ctx;
}
