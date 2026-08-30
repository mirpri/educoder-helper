// 浏览页：课程 → 作业 → 关卡 → 关卡详情。覆盖 CLI 的
// courses / homeworks / challenges / task / code / enter 六条命令。
import { useState } from "react";
import {
  ArrowRight,
  Copy,
  CornerDownRight,
  Download,
  FileCode2,
  PlayCircle,
  RefreshCw,
  ScrollText,
} from "lucide-react";

import * as api from "../api";
import { useApp } from "../context";
import { useAsync } from "../hooks";
import type { Challenge, Course, Homework } from "../types";
import { Badge, Breadcrumbs, Empty, ErrorBox, IdChip, Json, Loading, Spinner } from "../ui";

type Level =
  | { kind: "courses" }
  | { kind: "homeworks"; courseId: string; courseName: string }
  | { kind: "challenges"; shixunId: string; title: string }
  | { kind: "task"; gameId: string; title: string };

export default function Browse() {
  const { goto, select } = useApp();
  const [stack, setStack] = useState<Level[]>([{ kind: "courses" }]);
  const level = stack[stack.length - 1];

  const push = (next: Level) => setStack((s) => [...s, next]);
  const popTo = (index: number) => setStack((s) => s.slice(0, index + 1));

  const crumbs = stack.map((l, i) => ({
    label:
      l.kind === "courses"
        ? "课程"
        : l.kind === "homeworks"
          ? l.courseName
          : l.title,
    onClick: i < stack.length - 1 ? () => popTo(i) : undefined,
  }));

  return (
    <div className="page">
      <header className="page-head">
        <div className="page-head-row">
          <h1>浏览</h1>
          <JumpBox
            onChallenges={(id) => push({ kind: "challenges", shixunId: id, title: `实训 ${id}` })}
            onTask={(id) => push({ kind: "task", gameId: id, title: `关卡 ${id}` })}
          />
        </div>
        <Breadcrumbs items={crumbs} />
      </header>

      {level.kind === "courses" ? (
        <Courses
          onOpen={(c) => {
            select({ courseId: String(c.id), courseName: c.name });
            push({ kind: "homeworks", courseId: String(c.id), courseName: c.name ?? `课程 ${c.id}` });
          }}
          onExport={(c) => goto("export", { exportLevel: "course", courseId: String(c.id), courseName: c.name })}
        />
      ) : null}

      {level.kind === "homeworks" ? (
        <Homeworks
          courseId={level.courseId}
          onChallenges={(h) => {
            select({ homeworkName: h.name, shixunId: h.shixun_identifier, myshixunId: h.myshixun_identifier });
            push({
              kind: "challenges",
              shixunId: h.shixun_identifier!,
              title: h.name ?? `实训 ${h.shixun_identifier}`,
            });
          }}
          onReport={(h) => goto("report", { reportId: String(h.student_work_id) })}
          onExport={(h) =>
            goto("export", {
              exportLevel: "shixun",
              myshixunId: h.myshixun_identifier,
              shixunId: h.shixun_identifier,
              homeworkName: h.name,
            })
          }
        />
      ) : null}

      {level.kind === "challenges" ? (
        <Challenges
          shixunId={level.shixunId}
          onOpen={(c) => {
            select({ gameId: c.game_identifier });
            push({
              kind: "task",
              gameId: c.game_identifier!,
              title: `第${c.position}关 ${c.name ?? ""}`.trim(),
            });
          }}
        />
      ) : null}

      {level.kind === "task" ? <Task gameId={level.gameId} /> : null}
    </div>
  );
}

/** Direct id entry, for when the user already has a shixunId or gameId. */
function JumpBox({
  onChallenges,
  onTask,
}: {
  onChallenges: (id: string) => void;
  onTask: (id: string) => void;
}) {
  const [kind, setKind] = useState<"shixun" | "game">("shixun");
  const [value, setValue] = useState("");
  const submit = () => {
    const id = value.trim();
    if (!id) return;
    if (kind === "shixun") onChallenges(id);
    else onTask(id);
    setValue("");
  };
  return (
    <div className="jumpbox">
      <select className="input select" value={kind} onChange={(e) => setKind(e.target.value as "shixun" | "game")}>
        <option value="shixun">shixunId</option>
        <option value="game">gameId</option>
      </select>
      <input
        className="input mono"
        placeholder="直接输入标识符跳转"
        value={value}
        onChange={(e) => setValue(e.target.value)}
        onKeyDown={(e) => e.key === "Enter" && submit()}
      />
      <button className="btn" onClick={submit} disabled={!value.trim()}>
        <ArrowRight size={14} /> 跳转
      </button>
    </div>
  );
}

function Guard({
  state,
  empty,
  children,
}: {
  state: { loading: boolean; error: api.ApiError | null; data: unknown };
  empty: React.ReactNode;
  children: React.ReactNode;
}) {
  const { goto } = useApp();
  if (state.loading) return <Loading />;
  if (state.error) return <ErrorBox error={state.error} onFixCookies={() => goto("account")} />;
  if (!state.data) return <>{empty}</>;
  return <>{children}</>;
}

// ---- 课程 ----

function Courses({ onOpen, onExport }: { onOpen: (c: Course) => void; onExport: (c: Course) => void }) {
  const state = useAsync(() => api.courses(), []);
  const list = state.data?.courses ?? [];

  return (
    <Guard state={state} empty={<Empty title="没有课程" />}>
      <div className="list-head">
        <span className="muted">共 {state.data?.count ?? list.length} 门课程</span>
        <button className="btn btn-ghost btn-sm" onClick={state.reload}>
          <RefreshCw size={13} /> 刷新
        </button>
      </div>
      {list.length === 0 ? (
        <Empty title="这个账号下没有课程" />
      ) : (
        <ul className="rows">
          {list.map((c) => (
            <li key={c.id} className="row row-click" onClick={() => onOpen(c)}>
              <div className="row-main">
                <div className="row-title">
                  {c.name}
                  {c.is_end ? <Badge tone="warn">已结束</Badge> : null}
                </div>
                <div className="row-meta">
                  {c.members_count != null ? <span>成员 {c.members_count}</span> : null}
                  {c.homework_commons_count != null ? <span>作业 {c.homework_commons_count}</span> : null}
                  <IdChip label="courseId" value={c.id} />
                </div>
              </div>
              <div className="row-actions">
                <button
                  className="btn btn-sm"
                  onClick={(e) => {
                    e.stopPropagation();
                    onExport(c);
                  }}
                >
                  <Download size={13} /> 导出全部实训
                </button>
              </div>
            </li>
          ))}
        </ul>
      )}
    </Guard>
  );
}

// ---- 作业 ----

function Homeworks({
  courseId,
  onChallenges,
  onReport,
  onExport,
}: {
  courseId: string;
  onChallenges: (h: Homework) => void;
  onReport: (h: Homework) => void;
  onExport: (h: Homework) => void;
}) {
  // 4=实训作业, 1=图文作业 — the two `edu homeworks` types.
  const [kind, setKind] = useState(4);
  const state = useAsync(() => api.homeworks(courseId, kind), [courseId, kind]);
  const list = state.data?.homeworks ?? [];

  return (
    <>
      <div className="tabs">
        <button className={`tab${kind === 4 ? " is-active" : ""}`} onClick={() => setKind(4)}>
          实训作业
        </button>
        <button className={`tab${kind === 1 ? " is-active" : ""}`} onClick={() => setKind(1)}>
          图文作业
        </button>
        <span className="tabs-spacer" />
        <button className="btn btn-ghost btn-sm" onClick={state.reload}>
          <RefreshCw size={13} /> 刷新
        </button>
      </div>

      <Guard state={state} empty={<Empty title="没有作业" />}>
        {list.length === 0 ? (
          <Empty title="这门课下没有该类型的作业" />
        ) : (
          <ul className="rows">
            {list.map((h) => {
              const status = Array.isArray(h.status) ? h.status.join(" / ") : h.status;
              const done = h.finished_challenge_count ?? 0;
              const total = h.challenge_count;
              return (
                <li key={h.id ?? h.name} className="row">
                  <div className="row-main">
                    <div className="row-title">
                      {h.name}
                      {status ? <Badge>{status}</Badge> : null}
                      {total != null ? (
                        <Badge tone={done >= total ? "ok" : "neutral"}>
                          {done}/{total} 关
                        </Badge>
                      ) : null}
                    </div>
                    <div className="row-meta">
                      {h.end_time ? <span>截止 {h.end_time}</span> : null}
                      <IdChip label="shixunId" value={h.shixun_identifier} />
                      <IdChip label="myshixunId" value={h.myshixun_identifier} />
                      <IdChip label="reportId" value={h.student_work_id} />
                    </div>
                  </div>
                  <div className="row-actions">
                    {h.shixun_identifier ? (
                      <button className="btn btn-sm btn-primary" onClick={() => onChallenges(h)}>
                        <CornerDownRight size={13} /> 关卡
                      </button>
                    ) : null}
                    {h.student_work_id ? (
                      <button className="btn btn-sm" onClick={() => onReport(h)}>
                        <ScrollText size={13} /> 报告
                      </button>
                    ) : null}
                    {h.myshixun_identifier ? (
                      <button className="btn btn-sm" onClick={() => onExport(h)}>
                        <Download size={13} /> 导出
                      </button>
                    ) : null}
                  </div>
                </li>
              );
            })}
          </ul>
        )}
      </Guard>
    </>
  );
}

// ---- 关卡列表 ----

function Challenges({ shixunId, onOpen }: { shixunId: string; onOpen: (c: Challenge) => void }) {
  const { goto } = useApp();
  const state = useAsync(() => api.challenges(shixunId), [shixunId]);
  const [entering, setEntering] = useState(false);
  const [enterError, setEnterError] = useState<api.ApiError | null>(null);
  const list = state.data?.challenge_list ?? [];
  const hasInstance = list.some((c) => c.game_identifier);

  async function enter() {
    setEntering(true);
    setEnterError(null);
    try {
      await api.enterShixun(shixunId);
      state.reload();
    } catch (e) {
      setEnterError(api.toApiError(e));
    } finally {
      setEntering(false);
    }
  }

  return (
    <Guard state={state} empty={<Empty title="没有关卡" />}>
      <div className="list-head">
        <span className="muted">共 {list.length} 关</span>
        <button className="btn btn-ghost btn-sm" onClick={state.reload}>
          <RefreshCw size={13} /> 刷新
        </button>
      </div>

      {!hasInstance ? (
        <div className="notice notice-warn">
          <div>
            <strong>尚未进入该实训</strong>
            <div className="muted small">
              没有实例就没有 gameId，无法查看任务详情或导出代码。「进入实训」会在服务器上创建一个新实例
              （若此前重置过，会基于模板新建）。
            </div>
          </div>
          <button className="btn btn-primary btn-sm" onClick={() => void enter()} disabled={entering}>
            {entering ? <Spinner /> : <PlayCircle size={14} />} 进入实训
          </button>
        </div>
      ) : null}
      {enterError ? <ErrorBox error={enterError} onFixCookies={() => goto("account")} /> : null}

      <ul className="rows">
        {list.map((c) => (
          <li
            key={c.position}
            className={`row${c.game_identifier ? " row-click" : ""}`}
            onClick={() => c.game_identifier && onOpen(c)}
          >
            <div className="row-main">
              <div className="row-title">
                <span className="pos">第{c.position}关</span>
                {c.name}
                {c.finish_status ? <Badge tone="ok">已完成</Badge> : null}
              </div>
              <div className="row-meta">
                {c.score != null ? <span>{c.score} 分</span> : null}
                {c.passed_count != null ? <span>通过 {c.passed_count} 人</span> : null}
                <IdChip label="gameId" value={c.game_identifier} />
              </div>
            </div>
          </li>
        ))}
      </ul>
    </Guard>
  );
}

// ---- 关卡详情 ----

function Task({ gameId }: { gameId: string }) {
  const { goto, select } = useApp();
  const state = useAsync(() => api.task(gameId), [gameId]);
  const [showRaw, setShowRaw] = useState(false);
  const ch = state.data?.challenge;
  // `path` lists the editable files, separated by ; or ；.
  const files = (ch?.path ?? "")
    .split(/[；;]/)
    .map((s) => s.trim())
    .filter(Boolean);

  return (
    <Guard state={state} empty={<Empty icon={<FileCode2 size={30} strokeWidth={1.5} />} title="没有关卡详情" />}>
      <section className="card">
        <div className="card-head">
          <h2>
            第{ch?.position}关 {ch?.subject}
          </h2>
          <div className="row-actions">
            <button
              className="btn btn-sm"
              onClick={() => {
                select({ gameId, myshixunId: state.data?.myshixun?.identifier });
                goto("export", { exportLevel: "challenge", gameId });
              }}
            >
              <Download size={13} /> 导出这一关
            </button>
            <button className="btn btn-ghost btn-sm" onClick={() => setShowRaw((v) => !v)}>
              {showRaw ? "隐藏" : "查看"} JSON
            </button>
          </div>
        </div>
        <div className="row-meta">
          {state.data?.shixun?.name ? <span>{state.data.shixun.name}</span> : null}
          {ch?.score != null ? <span>{ch.score} 分</span> : null}
          {ch?.difficulty != null ? <span>难度 {ch.difficulty}</span> : null}
          {state.data?.game?.final_score != null ? <span>得分 {state.data.game.final_score}</span> : null}
          <IdChip label="gameId" value={gameId} />
          <IdChip label="myshixunId" value={state.data?.myshixun?.identifier} />
        </div>
        {showRaw ? <Json value={state.data} /> : null}
      </section>

      <section className="card">
        <div className="card-head">
          <h2>任务描述</h2>
          {ch?.task_pass ? (
            <button
              className="btn btn-ghost btn-sm"
              onClick={() => void navigator.clipboard.writeText(ch.task_pass!)}
            >
              <Copy size={13} /> 复制 Markdown
            </button>
          ) : null}
        </div>
        {ch?.task_pass ? (
          <pre className="prose">{ch.task_pass}</pre>
        ) : (
          <p className="muted">这一关没有任务描述。</p>
        )}
      </section>

      <section className="card">
        <div className="card-head">
          <h2>可编辑文件</h2>
        </div>
        {files.length === 0 ? (
          <p className="muted">这一关没有列出可编辑文件。</p>
        ) : (
          <FileViewer gameId={gameId} files={files} />
        )}
      </section>
    </Guard>
  );
}

/** Tabs over `challenge.path`, each showing the repo's current file content. */
function FileViewer({ gameId, files }: { gameId: string; files: string[] }) {
  const [active, setActive] = useState(files[0]);
  const state = useAsync(() => api.fileContent(gameId, active), [gameId, active]);

  return (
    <>
      <div className="tabs tabs-files">
        {files.map((f) => (
          <button
            key={f}
            className={`tab mono${f === active ? " is-active" : ""}`}
            onClick={() => setActive(f)}
            title={f}
          >
            {f}
          </button>
        ))}
        <span className="tabs-spacer" />
        {state.data != null ? (
          <button
            className="btn btn-ghost btn-sm"
            onClick={() => void navigator.clipboard.writeText(state.data!)}
          >
            <Copy size={13} /> 复制
          </button>
        ) : null}
      </div>
      {state.loading ? <Loading label="读取仓库文件…" /> : null}
      {state.error ? <ErrorBox error={state.error} /> : null}
      {state.data != null ? <pre className="code-block code-file">{state.data}</pre> : null}
    </>
  );
}
