// 报告页：对应 `edu report <reportId>` —— 每关分数、经验与改动文件。
import { useEffect, useState } from "react";

import { Braces, ScrollText, Search } from "lucide-react";

import * as api from "../api";
import { useApp } from "../context";
import { useAsync } from "../hooks";
import { Badge, Empty, ErrorBox, IdChip, Json, Loading } from "../ui";

export default function Report() {
  const { goto, selection } = useApp();
  const [input, setInput] = useState(selection.reportId ?? "");
  const [reportId, setReportId] = useState(selection.reportId ?? "");
  const [showRaw, setShowRaw] = useState(false);

  // Arriving from 浏览 with a reportId in hand: load it straight away.
  useEffect(() => {
    if (selection.reportId) {
      setInput(selection.reportId);
      setReportId(selection.reportId);
    }
  }, [selection.reportId]);

  const state = useAsync(() => api.report(reportId), [reportId], reportId !== "");
  const d = state.data;
  const stages = d?.stage_list ?? [];
  // `shixun_detail` holds the submitted files, one entry per challenge worked
  // on; match it to a stage by challenge_id.
  const filesByChallenge = new Map(
    (d?.shixun_detail ?? []).map((s) => [
      s.challenge_id,
      (s.game_codes ?? []).map((g) => g.path || g.filename).filter(Boolean) as string[],
    ]),
  );

  return (
    <div className="page">
      <header className="page-head">
        <h1>作业分数</h1>
        <p className="muted">
          reportId 就是作业列表里的 <span className="mono">student_work_id</span>，在「浏览 → 作业」
          里点「分数」会自动带过来。
        </p>
        <div className="input-row input-row-wide">
          <input
            className="input mono"
            value={input}
            onChange={(e) => setInput(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && setReportId(input.trim())}
            placeholder="reportId"
            spellCheck={false}
          />
          <button className="btn btn-primary" onClick={() => setReportId(input.trim())} disabled={!input.trim()}>
            <Search size={14} /> 查询
          </button>
        </div>
      </header>

      {!reportId ? (
        <Empty icon={<ScrollText size={30} strokeWidth={1.5} />} title="输入 reportId 查看报告" />
      ) : null}
      {reportId && state.loading ? <Loading /> : null}
      {state.error ? <ErrorBox error={state.error} onFixCookies={() => goto("account")} /> : null}

      {d ? (
        <>
          <section className="card">
            <div className="card-head">
              <h2>{d.homework_name ?? "作业报告"}</h2>
              <button className="btn btn-ghost btn-sm" onClick={() => setShowRaw((v) => !v)}>
                <Braces size={13} /> {showRaw ? "隐藏" : "查看"} JSON
              </button>
            </div>
            <div className="row-meta">
              {d.course_name ? <span>课程 {d.course_name}</span> : null}
              {d.work_score != null ? <span>得分 {d.work_score}</span> : null}
              {d.total_experience != null ? (
                <span>
                  经验 {d.myself_experience ?? 0}/{d.total_experience}
                </span>
              ) : null}
              {d.group_name ? <span>分组 {d.group_name}</span> : null}
              <IdChip label="reportId" value={reportId} />
            </div>
            {showRaw ? <Json value={d} /> : null}
          </section>

          <section className="card">
            <div className="card-head">
              <h2>关卡明细</h2>
              <span className="muted">共 {stages.length} 关</span>
            </div>
            {stages.length === 0 ? (
              <Empty title="报告里没有关卡明细" />
            ) : (
              <ul className="rows">
                {stages.map((s, i) => {
                  const files = filesByChallenge.get(s.challenge_id) ?? [];
                  return (
                    <li key={i} className="row">
                      <div className="row-main">
                        <div className="row-title">
                          <span className="pos">{s.challenge_num ?? i + 1}</span>
                          {s.name}
                          {s.game_score != null ? (
                            <Badge tone={s.game_score > 0 ? "ok" : "neutral"}>{s.game_score} 分</Badge>
                          ) : null}
                        </div>
                        <div className="row-meta">
                          {s.experience != null ? <span>满分经验 {s.experience}</span> : null}
                          {s.diff_code_count ? <span>改动 {s.diff_code_count}</span> : null}
                          {s.finished_time && s.finished_time !== "--" ? (
                            <span>完成于 {s.finished_time}</span>
                          ) : null}
                        </div>
                        {files.length > 0 ? (
                          <div className="row-files mono">改动文件：{files.join("，")}</div>
                        ) : null}
                      </div>
                    </li>
                  );
                })}
              </ul>
            )}
          </section>
        </>
      ) : null}
    </div>
  );
}
