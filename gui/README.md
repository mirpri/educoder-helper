# EduCoder Helper GUI

[Tauri 2](https://tauri.app) 桌面应用：React + TypeScript 前端，Rust 后端。
打包产物是单个可执行文件，最终用户不需要安装 Node 或 Rust。

## 开发

需要 [Rust 工具链](https://rustup.rs) 与 Node ≥ 18，以及 Tauri 的
[系统依赖](https://tauri.app/start/prerequisites/)（Windows 上是 WebView2 + MSVC 构建工具，
Linux 上是 `webkit2gtk` 等）。

```bash
npm install
npm start          # tauri dev：Vite 热更新 + Rust 后端
npm run bundle     # tauri build：产出安装包
npm run build      # 仅前端类型检查 + 构建
cd src-tauri && cargo test    # 后端单元测试
```

打包产物：

- `src-tauri/target/release/educoder-helper-gui.exe` —— 单文件可执行程序，无外部依赖
  （只要求系统有 WebView2 运行时，Windows 10/11 默认自带），可直接拷贝分发。
- `src-tauri/target/release/bundle/nsis/*-setup.exe` —— NSIS 安装程序。

架构跟随构建机。交叉出包：`rustup target add x86_64-pc-windows-msvc` 后
`npm run bundle -- --target x86_64-pc-windows-msvc`。

## 架构

```
src/                       前端（React 19 + TS，无 UI 框架，手写 CSS）
  api.ts                   invoke 的类型化封装，每个 CLI 命令一个函数
  types.ts                 API 响应与后端返回值的类型
  context.tsx              全局状态：登录态、当前页、页面间传递的标识符
  hooks.ts                 useAsync：加载/错误/竞态处理
  ui.tsx                   通用小组件（Badge、IdChip、ErrorBox…）
  pages/                   账号 / 浏览 / 导出 / 报告 / API 五个页面
  styles.css               全部样式，亮暗主题跟随系统

src-tauri/src/             后端（Rust）
  sign.rs                  签名：md5(base64("method=…&ak=…&sk=…&time=…"))
  cookies.rs               cookies.txt 解析 / 生成、路径解析
  client.rs                EduClient：签名请求、服务器时钟对齐、各接口封装
  exporter.rs              challenge / shixun / course 三级导出
  state.rs                 Cookie 状态、客户端实例、配置文件读写
  commands.rs              #[tauri::command]，前端可调用的全部命令
  error.rs                 统一错误类型，序列化后直接给前端
```

## 与 Node 实现的关系

`src-tauri/src/` 是仓库根目录 `src/*.js` 的逐文件移植，行为对齐到可测的程度：

| Rust | Node | 说明 |
| --- | --- | --- |
| `sign.rs` | `src/sign.js` | 同一套 ak/sk 与签名算法 |
| `cookies.rs` | `src/cookies.js` | 解析逻辑一致，另支持粘贴 `Cookie:` 请求头并写回 cookies.txt |
| `client.rs` | `src/client.js` | 同样的请求头、时钟对齐与接口封装 |
| `exporter.rs` | `src/exporter.js` | 同样的目录结构、文件名清洗规则与图片改写 |

`cargo test` 里的用例锁定了两处最容易漂移的行为——签名结果和 `sanitize()`
的文件名清洗（期望值取自 Node 实现的实际输出），**修改任一侧时请同步另一侧并跑测试**。

比 CLI 多出的行为，都是 GUI 场景下必要的：

- **浏览器登录**（`commands.rs` 的 `open_login_window` / `watch_login`）：打开一个指向
  educoder.net 登录页的 webview 窗口，登录成功后直接从该窗口的 Cookie store 里取出
  `_educoder_session`，写成 cookies.txt 并关闭窗口。轮询跑在 `spawn_blocking` 线程上——
  在 Windows 上从 UI 线程读 Cookie 会让 WebView2 死锁（wry#583）。结果通过
  `login:success` / `login:closed` / `login:timeout` / `login:error` 事件回报。
- 导出时支持取消（`Progress` 中的 `AtomicBool`），并通过 `export:log` 事件流式输出进度。
- 任务描述里的 `/api/attachments/…` 是站内相对路径，落到磁盘上就是裂图。`ImageMode`
  决定怎么修：`Link` 改写成绝对 URL，`Download` 抓到该关的 `images/` 再改成相对路径，
  `Keep` 不动。这些附件不需要登录也能取，两种修法都成立。扫描不用正则（避免多一个
  依赖）：`attachment_spans` 手工定位每个 URL 的字节区间，`splice` 一次性重建字符串——
  逐个 `replace` 会让短 id 命中长 id 替换结果的内部。
- 导出写文件时拒绝越界路径（`safe_join`），避免接口返回的路径写到目标目录之外。
- 返回 HTML 而非 JSON 时识别为登录态过期，提示重新导入 Cookie，而不是把 HTML 当数据展示。

## 前后端约定

- 命令名与 Rust 函数名一致（`load_cookies_file`）；参数名由 Tauri 自动在
  snake_case ↔ camelCase 之间转换（`course_id` ↔ `courseId`）。
- 所有错误都是 `error.rs` 里的 `Error`，前端按 `ApiError` 处理；`needsCookies`
  为真时界面会给出「去登录」的入口。
- 导出进度通过 `export:log`（每行一条）与 `export:done` 事件推送；
  登录结果通过 `login:*` 事件推送。
- 图标统一使用 [lucide-react](https://lucide.dev)，不要再混用 Unicode 字形。
