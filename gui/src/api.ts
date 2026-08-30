// Thin typed wrappers over the Rust commands. One function per CLI verb.
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

import type {
  ChallengeResult,
  ChallengesResponse,
  CookieStatus,
  CourseResult,
  CoursesResponse,
  HomeworksResponse,
  RawResponse,
  ReportResponse,
  ShixunResult,
  TaskResponse,
  UserInfo,
} from "./types";

/** The serialised `crate::error::Error`. */
export interface ApiError {
  message: string;
  status?: number;
  data?: unknown;
  /** True when the fix is "import cookies again", so the UI can offer that. */
  needsCookies: boolean;
}

export function isApiError(e: unknown): e is ApiError {
  return typeof e === "object" && e !== null && "message" in e && "needsCookies" in e;
}

/** Anything thrown out of `invoke` becomes an `ApiError` so callers see one shape. */
export function toApiError(e: unknown): ApiError {
  if (isApiError(e)) return e;
  return { message: e instanceof Error ? e.message : String(e), needsCookies: false };
}

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(cmd, args);
  } catch (e) {
    throw toApiError(e);
  }
}

// ---- Account ----

export const cookieStatus = () => call<CookieStatus>("cookie_status");
export const loadCookiesFile = (path: string) => call<CookieStatus>("load_cookies_file", { path });
export const loadCookiesText = (text: string) => call<CookieStatus>("load_cookies_text", { text });
export const exportCookiesFile = (path: string) => call<string>("export_cookies_file", { path });
export const clearCookies = () => call<CookieStatus>("clear_cookies");

// ---- Browser login ----

/**
 * Opens a webview window on the EduCoder login page. The backend watches that
 * window's cookie store and fires `login:success` once the user has signed in,
 * so nothing has to be copied by hand.
 */
export const openLoginWindow = () => call<void>("open_login_window");
export const closeLoginWindow = () => call<void>("close_login_window");

export type LoginEvent =
  | { kind: "success"; status: CookieStatus }
  | { kind: "closed" }
  | { kind: "timeout" }
  | { kind: "error"; error: ApiError };

/** Subscribes to every outcome of the login window; returns an unlisten function. */
export async function onLogin(fn: (e: LoginEvent) => void): Promise<() => void> {
  const offs = await Promise.all([
    listen<CookieStatus>("login:success", (e) => fn({ kind: "success", status: e.payload })),
    listen("login:closed", () => fn({ kind: "closed" })),
    listen("login:timeout", () => fn({ kind: "timeout" })),
    listen<ApiError>("login:error", (e) => fn({ kind: "error", error: e.payload })),
  ]);
  return () => offs.forEach((off) => off());
}

// ---- Queries (mirroring `edu <command>`) ----

/** `edu me` */
export const me = () => call<UserInfo>("me");

/** `edu courses [login]` */
export const courses = (login?: string) => call<CoursesResponse>("courses", { login: login || null });

/** `edu homeworks <courseId> [type]` — 4=实训作业, 1=图文作业. */
export const homeworks = (courseId: string, kind = 4) =>
  call<HomeworksResponse>("homeworks", { courseId, kind });

/** `edu challenges <shixunId>` */
export const challenges = (shixunId: string) =>
  call<ChallengesResponse>("challenges", { shixunId });

/** `edu task <gameId>` */
export const task = (gameId: string) => call<TaskResponse>("task", { gameId });

/** `edu code <gameId> <path>` */
export const fileContent = (gameId: string, path: string) =>
  call<string>("file_content", { gameId, path });

/** `edu report <reportId>` */
export const report = (reportId: string) => call<ReportResponse>("report", { reportId });

/** `edu enter <shixunId>` — creates an instance when none exists. */
export const enterShixun = (shixunId: string) =>
  call<{ game_identifier?: string }>("enter_shixun", { shixunId });

/** `edu get/post/raw` */
export const rawRequest = (method: string, path: string, body?: string) =>
  call<RawResponse>("raw_request", { method, path, body: body || null });

// ---- Export ----

/** `edu export challenge <gameId> [dir]` */
export const exportChallenge = (gameId: string, dest: string, name?: string) =>
  call<ChallengeResult>("export_challenge", { gameId, dest, name: name || null });

/** `edu export shixun <myshixunId> [dir]` */
export const exportShixun = (myshixunId: string, dest: string, name?: string) =>
  call<ShixunResult>("export_shixun", { myshixunId, dest, name: name || null });

/** `edu export course <courseId> [dir]` */
export const exportCourse = (
  courseId: string,
  dest: string,
  name?: string,
  enterIfNeeded = true,
) => call<CourseResult>("export_course", { courseId, dest, name: name || null, enterIfNeeded });

export const cancelExport = () => call<void>("cancel_export");
export const isExporting = () => call<boolean>("is_exporting");

/** Subscribes to the export progress log; returns an unlisten function. */
export const onExportLog = (fn: (line: string) => void) =>
  listen<string>("export:log", (e) => fn(e.payload));
