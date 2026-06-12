#!/usr/bin/env node
// EduCoder 命令行工具。
import fs from 'node:fs';
import { EduClient } from '../src/client.js';
import { exportChallenge, exportShixun, exportCourse } from '../src/exporter.js';
import * as pretty from '../src/pretty.js';

const USAGE = `edu - EduCoder API 命令行工具

用法:
  edu me                              查看当前登录用户。
  edu courses [login]                 列出你的课程。
  edu homeworks <courseId> [type]     列出作业（type: 4=实训[默认], 1=图文）。
  edu challenges <shixunId>           列出实训的关卡；若已有实例，附带每关的 gameId。
  edu task <gameId> [--desc]          查看关卡详情；--desc 只输出任务描述 Markdown。
  edu code <gameId> <path>            输出仓库文件的当前内容（已解码）。
  edu report <reportId>               查看作业报告（每关分数/改动文件）。
  edu enter <shixunId>                进入实训，输出第一关的 gameId。
                                      （若无实例则会新建一个实例。）
  edu export challenge <gameId> [dir]      导出单个关卡：README.md + 可编辑文件。
  edu export shixun <myshixunId> [dir]     导出整个实训（每关一个子目录）。
  edu export course <courseId> [dir]       导出课程的所有实训作业。
                                      [dir] 默认为导出对象的名称。
  edu get <path|url>                  原始签名 GET 请求，输出 JSON。
  edu post <path|url> [bodyFile]      原始签名 POST 请求（请求体取自文件或 stdin）。
  edu raw <METHOD> <path|url> [bodyFile]
                                      原始签名请求，可指定任意方法。

选项:
  --cookies <file>   cookies.txt 路径（否则取 $EDUCODER_COOKIES / ./cookies.txt）。
  --pretty           输出人类可读的中文摘要（默认）。
  --raw              输出完整 JSON；对 get/post/raw 则为原始响应体。

示例:
  edu courses
  edu homeworks 113636
  edu get /api/users/get_user_info.json
`;

function parseArgs(argv) {
  const opts = { cookies: undefined, raw: false };
  const rest = [];
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (a === '--cookies') opts.cookies = argv[++i];
    else if (a === '--raw') opts.raw = true;
    else if (a === '--pretty') opts.raw = false;
    else if (a === '--desc') opts.desc = true;
    else if (a === '-h' || a === '--help') opts.help = true;
    else rest.push(a);
  }
  return { opts, rest };
}

function out(data) {
  if (typeof data === 'string') process.stdout.write(data + (data.endsWith('\n') ? '' : '\n'));
  else process.stdout.write(JSON.stringify(data, null, 2) + '\n');
}

function readBody(file) {
  if (file) return fs.readFileSync(file, 'utf8');
  if (!process.stdin.isTTY) return fs.readFileSync(0, 'utf8'); // piped stdin
  return undefined;
}

async function main() {
  const { opts, rest } = parseArgs(process.argv.slice(2));
  const [cmd, ...args] = rest;
  if (!cmd || opts.help) { process.stdout.write(USAGE); return; }

  const client = new EduClient({ cookiesPath: opts.cookies });
  const reqOpts = opts.raw ? { raw: true } : {};
  const show = (r) => out(opts.raw ? r.body : r);
  // Convenience commands: --raw prints the full JSON, else a human-readable summary.
  const present = (data, fmt) => out(opts.raw ? data : fmt(data));

  switch (cmd) {
    case 'me':
      present(await client.getUserInfo(), pretty.me); break;
    case 'courses':
      present(await client.getCourses(args[0]), pretty.courses); break;
    case 'homeworks': {
      if (!args[0]) throw new Error('homeworks needs a <courseId>');
      present(await client.getHomeworks(args[0], { type: args[1] ? Number(args[1]) : 4 }), pretty.homeworks);
      break;
    }
    case 'challenges': {
      if (!args[0]) throw new Error('challenges needs a <shixunId>');
      const data = await client.getChallenges(args[0]);
      // Attach each challenge's gameId from the caller's EXISTING instance, if any.
      // getShixun is read-only (myshixun_id===0 means no instance) so this
      // never creates a new instance the way `enter` would.
      const myId = (await client.getShixun(args[0]).catch(() => null))?.myshixun_id;
      if (myId) {
        const mine = await client.getMyChallenges(myId).catch(() => []);
        const byPos = new Map(mine.map((g) => [g.position, g.identifier]));
        for (const c of (data.challenge_list || [])) c.game_identifier = byPos.get(c.position);
      }
      present(data, pretty.challenges);
      break;
    }
    case 'task': {
      if (!args[0]) throw new Error('task needs a <gameId>');
      const t = await client.getTask(args[0]);
      if (opts.desc) out(t?.challenge?.task_pass ?? '(no task_pass)');
      else present(t, pretty.task);
      break;
    }
    case 'code':
      if (!args[0] || !args[1]) throw new Error('code needs <gameId> <path>');
      out(await client.getFileContent(args[0], args[1])); break;
    case 'report':
      if (!args[0]) throw new Error('report needs a <reportId>');
      present(await client.getWorkReport(args[0]), pretty.report); break;
    case 'enter':
      if (!args[0]) throw new Error('enter needs a <shixunId>');
      present(await client.enterShixun(args[0]), pretty.enter); break;
    case 'export': {
      const [level, id, dir] = args;
      const log = (m) => process.stderr.write(m + '\n');
      if (level === 'challenge') {
        if (!id) throw new Error('export challenge needs <gameId>');
        const r = await exportChallenge(client, id, dir, log);
        present({ exported: r.dir, ...r }, pretty.exportChallenge);
      } else if (level === 'shixun') {
        if (!id) throw new Error('export shixun needs <myshixunId>');
        const r = await exportShixun(client, id, dir, log);
        present({ exported: r.dir, challenges: r.challenges.length }, pretty.exportShixun);
      } else if (level === 'course') {
        if (!id) throw new Error('export course needs <courseId>');
        const r = await exportCourse(client, id, dir, {}, log);
        present({ exported: r.dir, summary: r.summary }, pretty.exportCourse);
      } else {
        throw new Error('export level must be: challenge | shixun | course');
      }
      break;
    }
    case 'get':
      if (!args[0]) throw new Error('get needs a <path|url>');
      show(await client.request('GET', args[0], reqOpts)); break;
    case 'post':
      if (!args[0]) throw new Error('post needs a <path|url>');
      show(await client.request('POST', args[0], { ...reqOpts, body: readBody(args[1]) })); break;
    case 'raw': {
      const [method, path, bodyFile] = args;
      if (!method || !path) throw new Error('raw needs <METHOD> <path|url>');
      show(await client.request(method, path, { ...reqOpts, body: readBody(bodyFile) }));
      break;
    }
    default:
      process.stderr.write(`Unknown command: ${cmd}\n\n${USAGE}`);
      process.exit(2);
  }
}

main().catch((e) => {
  process.stderr.write(`ERROR: ${e.message}\n`);
  if (e.data) process.stderr.write(JSON.stringify(e.data, null, 2) + '\n');
  process.exit(1);
});
