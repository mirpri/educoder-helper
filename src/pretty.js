// Human-readable (Chinese) formatters for the CLI's default --pretty mode.
// Each function takes a parsed API response and returns a string. They are
// defensive: missing fields are skipped, and an unexpected top-level shape
// falls back to indented JSON so no information is silently lost.

const J = (d) => JSON.stringify(d, null, 2);

// "标签: 值" — returns '' (skipped) when the value is null/undefined/''.
function kv(label, value) {
  if (value === undefined || value === null || value === '') return '';
  return `${label}: ${value}`;
}

// Join the non-empty parts with newlines.
function lines(...parts) {
  return parts.filter(Boolean).join('\n');
}

// One item-header line: "idLabel: id [status] title — meta".
// Pass id=null to omit the leading identifier; empty parts are dropped.
function header(idLabel, id, status, title, meta) {
  const lead = id != null && id !== '' ? `${idLabel}: ${id}` : '';
  return [lead, status, title].filter(Boolean).join(' ') + (meta ? ' - ' + meta : '');
}

export function me(d) {
  if (!d || typeof d !== 'object') return J(d);
  const name = d.real_name
    ? (d.username ? `${d.real_name} (${d.username})` : d.real_name)
    : d.username;
  return lines(
    kv('姓名', name),
    kv('登录名', d.login),
    kv('学号', d.student_id),
    kv('身份', d.user_identity),
  );
}

export function courses(d) {
  const list = Array.isArray(d) ? d : d && d.courses;
  if (!Array.isArray(list)) return J(d);
  const head = `共 ${d.count ?? list.length} 门课程`;
  const rows = list.map((c) => {
    const meta = [
      c.members_count != null ? `成员${c.members_count}` : '',
      c.homework_commons_count != null ? `作业${c.homework_commons_count}` : '',
    ].filter(Boolean).join(' ');
    return header('courseId', c.id, c.is_end ? '[已结束]' : '', c.name, meta);
  });
  return lines(head, ...rows);
}

export function homeworks(d) {
  const list = d && d.homeworks;
  if (!Array.isArray(list)) return J(d);
  const head = `共 ${list.length} 个作业`;
  const rows = list.map((h) => {
    const status = Array.isArray(h.status) ? h.status.join('/') : h.status;
    const progress = h.challenge_count != null
      ? `${h.finished_challenge_count ?? 0}/${h.challenge_count}` : '';
    const badge = [status, progress].filter(Boolean).join(', ');
    const meta = h.end_time ? `截止${h.end_time}` : '';
    const ids = [
      kv('shixunId', h.shixun_identifier),
      kv('myshixunId', h.myshixun_identifier),
      kv('reportId', h.student_work_id),
    ].filter(Boolean).join('\t');
    return lines('  ' + header(null, null, badge ? `[${badge}]` : '', h.name, meta), ids && '    ' + ids);
  });
  return lines(head, ...rows);
}

export function challenges(d) {
  const list = d && d.challenge_list;
  if (!Array.isArray(list)) return J(d);
  const head = `共 ${list.length} 关`;
  const rows = list.map((c) => {
    const meta = [
      c.score != null ? `${c.score}分` : '',
      c.passed_count != null ? `通过${c.passed_count}人` : '',
    ].filter(Boolean).join(' ');
    return header('gameId', c.game_identifier, c.finish_status ? '[已完成]' : '',
      `第${c.position}关 ${c.name}`, meta);
  });
  const note = list.some((c) => c.game_identifier)
    ? '' : '（尚未进入实训，无 gameId；用 enter <shixunId> 创建实例）';
  return lines(head, ...rows, note);
}

export function task(d) {
  const ch = d && d.challenge;
  if (!ch || typeof ch !== 'object') return J(d);
  const descLen = ch.task_pass ? String(ch.task_pass).length : 0;
  return lines(
    kv('实训', d.shixun && d.shixun.name),
    `第${ch.position}关 ${ch.subject}` + (ch.score != null ? ` - ${ch.score}分` : '')
      + (ch.difficulty != null ? ` 难度${ch.difficulty}` : ''),
    kv('可编辑文件', ch.path),
    kv('状态', d.game && d.game.status),
    kv('得分', d.game && d.game.final_score),
    kv('myshixun', d.myshixun && d.myshixun.identifier),
    kv('上一关', d.prev_game),
    kv('下一关', d.next_game),
    descLen ? `任务描述: ${descLen} 字（--desc 查看全文，--raw 查看 JSON）` : '',
  );
}

export function report(d) {
  const list = d && d.stage_list;
  if (!Array.isArray(list)) return J(d);
  // shixun_detail holds the student's submitted files, one entry per challenge
  // worked on; match it to a stage by challenge_id.
  const detail = Array.isArray(d.shixun_detail) ? d.shixun_detail : [];
  const byChallenge = new Map(detail.map((s) => [s.challenge_id, s]));
  const head = lines(
    `作业: ${d.homework_name}` + (d.course_name ? ` (课程: ${d.course_name})` : ''),
    kv('得分', d.work_score),
    d.total_experience != null ? `经验: ${d.myself_experience ?? 0}/${d.total_experience}` : '',
    kv('分组', d.group_name),
    `共 ${list.length} 关`,
  );
  const rows = list.map((s) => {
    const meta = [
      s.game_score != null ? `得分${s.game_score}` : '',
      s.experience != null ? `满分经验${s.experience}` : '',
      s.diff_code_count ? `改动${s.diff_code_count}` : '',
      s.finished_time && s.finished_time !== '--' ? `完成于${s.finished_time}` : '',
    ].filter(Boolean).join(' ');
    const det = byChallenge.get(s.challenge_id);
    const files = det && Array.isArray(det.game_codes)
      ? det.game_codes.map((g) => g.path || g.filename).filter(Boolean) : [];
    return lines(
      `  ${(s.challenge_num || '')} ${s.name}${meta ? ' - ' + meta : ''}`.trimEnd(),
      files.length ? `    改动文件: ${files.join('，')}` : '',
    );
  });
  return lines(head, ...rows);
}

export function enter(d) {
  if (!d || typeof d !== 'object') return J(d);
  if (!d.game_identifier) return lines('已进入实训（响应中无 gameId）', J(d));
  return `已进入实训，第一关 gameId: ${d.game_identifier}`;
}

export function exportChallenge(r) {
  if (!r || typeof r !== 'object') return J(r);
  const files = Array.isArray(r.files) ? r.files : [];
  const images = Array.isArray(r.images) ? r.images : [];
  return lines(
    `已导出到 ${r.dir}，文件 ${files.length} 个：`,
    ...files.map((f) => '  ' + f),
    ...(images.length ? [`  images/（${images.length} 张图片）`] : []),
  );
}

export function exportShixun(r) {
  if (!r || typeof r !== 'object') return J(r);
  return `已导出 ${r.challenges} 关到 ${r.exported || r.dir}`;
}

export function exportCourse(r) {
  if (!r || typeof r !== 'object' || !Array.isArray(r.summary)) return J(r);
  const rows = r.summary.map((s) => {
    let detail;
    if (s.error) detail = `失败: ${s.error}`;
    else if (s.skipped) detail = `跳过: ${s.skipped}`;
    else detail = `${s.challenges} 关`;
    return `  ${s.name} - ${detail}`;
  });
  return lines(`已导出到 ${r.exported || r.dir}`, ...rows);
}
