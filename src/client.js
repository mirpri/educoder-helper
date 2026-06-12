// EduClient: authenticated client for the EduCoder (educoder.net) API.
import crypto from 'node:crypto';
import { loadCookies } from './cookies.js';
import { signature } from './sign.js';

const HOST = 'www.educoder.net';

export class EduClient {
  /**
   * @param {object} [opts]
   * @param {string} [opts.cookiesPath]  Path to a Netscape cookies.txt.
   * @param {string} [opts.host]         API host (default www.educoder.net).
   */
  constructor(opts = {}) {
    this.host = opts.host || HOST;
    const { cookies, file } = loadCookies(opts.cookiesPath);
    this.cookies = cookies;
    this.cookiesFile = file;
    this._clockOffset = null; // serverMs - localMs
  }

  // Aligns to the server clock once (the local PC clock is often skewed).
  async _serverNow() {
    if (this._clockOffset === null) {
      try {
        const r = await fetch(`https://${this.host}/`, { method: 'HEAD' });
        const d = r.headers.get('date');
        this._clockOffset = d ? new Date(d).getTime() - Date.now() : 0;
      } catch {
        this._clockOffset = 0;
      }
    }
    return Date.now() + this._clockOffset;
  }

  async _headers(method) {
    const t = await this._serverNow();
    const sig = signature(method, t);
    return {
      'User-Agent': 'Mozilla/5.0',
      'Accept': 'application/json, text/plain, */*',
      'X-EDU-Type': 'pc',
      'X-EDU-Timestamp': String(t),
      'X-EDU-Signature': sig,
      'Pc-Authorization': this.cookies._educoder_session,
      'X-Original-Protocol': 'https:',
      'X-Original-Host': this.host,
      'X-Original-Origin': `https://${this.host}`,
      'X-Request-Id': crypto.randomUUID(),
      'Referer': `https://${this.host}/`,
      'Cookie': Object.entries(this.cookies).map(([k, v]) => `${k}=${v}`).join('; '),
    };
  }

  /**
   * Low-level signed request. Returns the parsed Response.
   * @param {string} method  HTTP method.
   * @param {string} pathOrUrl  "/api/..." or a full https URL.
   * @param {object} [opts]  { body, raw }. body is JSON-stringified unless a
   *                         string/Buffer. raw:true returns text not JSON.
   */
  async request(method, pathOrUrl, opts = {}) {
    method = method.toUpperCase();
    const url = pathOrUrl.startsWith('http') ? pathOrUrl : `https://${this.host}${pathOrUrl}`;
    const headers = await this._headers(method);

    const init = { method, headers, redirect: 'follow' };
    if (opts.body !== undefined && opts.body !== null) {
      if (typeof opts.body === 'string' || Buffer.isBuffer(opts.body)) {
        init.body = opts.body;
      } else {
        init.body = JSON.stringify(opts.body);
      }
      headers['Content-Type'] = 'application/json';
    }

    const res = await fetch(url, init);
    const text = await res.text();
    if (opts.raw) {
      return { status: res.status, redirected: res.redirected, url: res.url, body: text };
    }
    let data;
    try { data = text ? JSON.parse(text) : null; }
    catch { data = text; }
    if (!res.ok) {
      const e = new Error(`HTTP ${res.status} for ${method} ${url}`);
      e.status = res.status; e.data = data;
      throw e;
    }
    return data;
  }

  get(p, opts) { return this.request('GET', p, opts); }
  post(p, body, opts) { return this.request('POST', p, { ...opts, body }); }
  put(p, body, opts) { return this.request('PUT', p, { ...opts, body }); }
  delete(p, opts) { return this.request('DELETE', p, opts); }

  // ---- Convenience wrappers for common endpoints ----

  getUserInfo() {
    return this.get('/api/users/get_user_info.json');
  }

  // login defaults to the authenticated user's login.
  async getCourses(login) {
    if (!login) login = (await this.getUserInfo()).login;
    return this.get(`/api/users/${login}/courses.json`);
  }

  // type: 1=图文作业, 4=实训作业 (others exist per course).
  getHomeworks(courseId, { type = 4, page = 1, limit = 50 } = {}) {
    return this.get(`/api/courses/${courseId}/homework_commons.json?type=${type}&page=${page}&limit=${limit}`);
  }

  // Challenges (关卡) of a shixun by its identifier (e.g. "gu8nbv56").
  getChallenges(shixunIdentifier) {
    return this.get(`/api/shixuns/${shixunIdentifier}/challenges.json`);
  }

  // Shixun detail by identifier. Includes myshixun_id: the numeric id of the
  // caller's EXISTING instance, or 0 if none. Read-only — does NOT create an
  // instance (unlike enterShixun). The numeric id works with getMyChallenges.
  getShixun(shixunIdentifier) {
    return this.get(`/api/shixuns/${shixunIdentifier}.json`);
  }

  // The student's instantiated challenges (games) for a myshixun_identifier.
  getMyChallenges(myshixunIdentifier) {
    return this.get(`/api/myshixuns/${myshixunIdentifier}/challenges.json`);
  }

  // A single challenge "game" by its game identifier. The task description /
  // requirements markdown is at result.challenge.task_pass.
  getTask(gameIdentifier) {
    return this.get(`/api/tasks/${gameIdentifier}.json`);
  }

  // Decoded text of a file in the student's repo (reads git HEAD = current code).
  // gameIdentifier: any task/game identifier of the shixun instance;
  // path: repo-relative path, e.g. "src/shell1.sh".
  async getFileContent(gameIdentifier, path) {
    const r = await this.get(`/api/tasks/${gameIdentifier}/rep_content.json?path=${encodeURIComponent(path)}`);
    const b64 = r && r.content && (r.content.content !== undefined ? r.content.content : r.content);
    if (b64 == null) { const e = new Error('no file content in response'); e.data = r; throw e; }
    return Buffer.from(b64, 'base64').toString('utf8');
  }

  // The student's work report for one submission (student_work_id): per-challenge scores,
  // time, diff_code_count, etc. (stage_list).
  getWorkReport(studentWorkId) {
    return this.get(`/api/student_works/${studentWorkId}/shixun_work_report.json`);
  }

  // Enter a shixun by identifier: instantiates a fresh myshixun/repo if none exists
  // and returns { game_identifier } of the first challenge.
  // NOTE: if a previous instance was reset, this creates a NEW instance from template.
  enterShixun(shixunIdentifier) {
    return this.get(`/api/shixuns/${shixunIdentifier}/shixun_exec.json`);
  }
}

export default EduClient;
