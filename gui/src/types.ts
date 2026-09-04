// Shapes of the EduCoder API responses, limited to the fields the UI reads.
// Everything is optional: the API is undocumented and fields come and go, so
// the components render what is present and skip the rest.

export interface UserInfo {
  login?: string;
  username?: string;
  real_name?: string;
  student_id?: string;
  user_identity?: string;
  image_url?: string;
}

export interface Course {
  id?: number;
  name?: string;
  is_end?: boolean;
  members_count?: number;
  homework_commons_count?: number;
}

export interface CoursesResponse {
  count?: number;
  courses?: Course[];
}

export interface Homework {
  id?: number;
  name?: string;
  status?: string[] | string;
  end_time?: string;
  is_shixun?: boolean;
  challenge_count?: number;
  finished_challenge_count?: number;
  shixun_identifier?: string;
  myshixun_identifier?: string;
  student_work_id?: number;
}

export interface HomeworksResponse {
  homeworks?: Homework[];
}

export interface Challenge {
  position?: number;
  name?: string;
  score?: number;
  passed_count?: number;
  finish_status?: number | boolean;
  game_identifier?: string;
}

export interface ChallengesResponse {
  challenge_list?: Challenge[];
  shixun_name?: string;
}

export interface TaskResponse {
  shixun?: { name?: string; identifier?: string };
  myshixun?: { identifier?: string };
  game?: { status?: number; final_score?: number };
  challenge?: {
    position?: number;
    subject?: string;
    score?: number;
    difficulty?: number;
    path?: string;
    task_pass?: string;
  };
  prev_game?: string;
  next_game?: string;
}

export interface Stage {
  challenge_id?: number;
  challenge_num?: number;
  name?: string;
  game_score?: number;
  experience?: number;
  diff_code_count?: number;
  finished_time?: string;
}

export interface ReportResponse {
  homework_name?: string;
  course_name?: string;
  work_score?: number;
  total_experience?: number;
  myself_experience?: number;
  group_name?: string;
  stage_list?: Stage[];
  shixun_detail?: { challenge_id?: number; game_codes?: { path?: string; filename?: string }[] }[];
}

// ---- Backend (Rust) types ----

export interface CookieStatus {
  loaded: boolean;
  source: string | null;
  names: string[];
  host: string;
}

export interface RawResponse {
  status: number;
  redirected: boolean;
  url: string;
  body: string;
}

/** 任务描述里 /api/attachments/... 图片的处理方式。 */
export type ImageMode = "keep" | "link" | "download";

export interface ChallengeResult {
  name: string | null;
  dir: string;
  files: string[];
  /** images: "download" 时保存到 images/ 的资源。 */
  images: string[];
}

export interface ChallengeEntry {
  position: number | null;
  name: string | null;
  files?: string[];
  error?: string;
}

export interface ShixunResult {
  dir: string;
  challenges: ChallengeEntry[];
}

export interface HomeworkEntry {
  name: string;
  challenges?: number;
  skipped?: string;
  error?: string;
}

export interface CourseResult {
  dir: string;
  summary: HomeworkEntry[];
}

export type ExportResult = ChallengeResult | ShixunResult | CourseResult;

// ---- AI 实验报告 ----

export interface TreeChallenge {
  position: number | null;
  name: string;
  gameId: string;
}

export interface TreeHomework {
  name: string;
  shixunId: string | null;
  myshixunId: string | null;
  challenges: TreeChallenge[];
  /** 为什么这个实训没有可选的关卡（未进入 / 读取失败）。 */
  skipped: string | null;
}

export interface ReportTree {
  courseName: string;
  homeworks: TreeHomework[];
}

export interface AiConfig {
  baseUrl: string;
  apiKey: string;
  model: string;
}

/** 由谁来写报告。 */
export type BackendKind = "api" | "claudeCode" | "codex";

export interface CliConfig {
  /** 可执行文件的完整路径；留空表示在 PATH 上找。 */
  path: string;
  /** 传给 CLI 的 --model；留空用它自己的默认模型。 */
  model: string;
}

export interface BackendConfig {
  kind: BackendKind;
  api: AiConfig;
  cli: CliConfig;
}

export interface AiSettings {
  config: BackendConfig;
  rememberApiKey: boolean;
}

/** 本机上探测到的两个 agent CLI。 */
export interface DetectedClis {
  claudeCode: string | null;
  codex: string | null;
}

export interface SelectedHomework {
  name: string;
  /** 该实训的关卡总数，供报告如实写"选取其中 N 个"。 */
  total: number;
  challenges: TreeChallenge[];
}

export interface ReportRequest {
  courseName: string;
  dest: string;
  folder?: string | null;
  homeworks: SelectedHomework[];
  taskBook: string;
  requirements: string;
  template: string;
}

export interface ReportResult {
  dir: string;
  file: string;
  sections: number;
  /** 生成失败、正文里留了提示的小节。 */
  failed: string[];
  /** 还需要人工补充截图 / 运行结果的处数。 */
  placeholders: number;
  materials: number;
}

// ---- AI 做实验 ----

export interface SolveRequest {
  courseName: string;
  dest: string;
  folder?: string | null;
  homeworks: SelectedHomework[];
  requirements: string;
}

export interface SolvedChallenge {
  name: string;
  dir: string;
  files: string[];
  error: string | null;
}

export interface SolveResult {
  dir: string;
  challenges: SolvedChallenge[];
  solved: number;
  failed: number;
  /** 汇总了所有关卡答案代码块的单个 md 文件，供直接复制。 */
  summaryFile: string;
}
