# EduCoder Helper

用于 [EduCoder](https://www.educoder.net)（头歌）平台的信息查询与导出工具。需要 Node ≥ 18，无第三方依赖。

## Cookies

为使用本工具，需要将 `educoder.net` 的 Cookie 导出为 Netscape 格式的 `cookies.txt`。
推荐使用浏览器扩展 [Get cookies.txt LOCALLY](https://chromewebstore.google.com/detail/get-cookiestxt-locally/cclelndahbckbenkjhflpdbgdldlbecc)：登录 `educoder.net` 后，在该站点页面点击扩展图标导出，将得到的文件保存为 `cookies.txt`。

文件按以下优先级查找：

1. `--cookies <file>`（CLI）/ `new EduClient({ cookiesPath })`（库）
2. `$EDUCODER_COOKIES` 环境变量
3. 项目根目录下的 `./cookies.txt`

Session Cookie 会定期过期，若请求返回登录重定向或 401，请重新导出。

## 命令行用法

```bash
# 基本查询
node bin/edu.js me                              # 查看当前登录用户
node bin/edu.js courses                         # 我的课程列表
node bin/edu.js homeworks <courseId>            # 实训作业列表（type 4）
node bin/edu.js homeworks <courseId> 1          # 图文作业列表（type 1）
node bin/edu.js challenges <shixunId>           # 某实训的关卡列表（已有实例时附带各关 gameId）
node bin/edu.js task <gameId> [--desc]          # 关卡详情；--desc 只输出任务描述 Markdown
node bin/edu.js code <gameId> <path>            # 读取仓库文件当前内容
node bin/edu.js report <reportId>               # 作业报告（每关分数/改动文件）
node bin/edu.js enter <shixunId>                # 进入实训，返回第一关 gameId

# 导出（存储代码和任务描述至本地）
node bin/edu.js export challenge <gameId> [dir]      # 导出单个挑战：README.md + 可编辑文件
node bin/edu.js export shixun <myshixunId> [dir]     # 导出整个实训（每个挑战一个子目录）
node bin/edu.js export course <courseId> [dir]       # 导出课程所有实训作业
                                                     # [dir] 默认为导出对象的名称

# 原始请求
node bin/edu.js get /api/users/get_user_info.json
node bin/edu.js post /api/some/endpoint body.json
node bin/edu.js raw PUT /api/some/endpoint body.json
echo '{"k":1}' | node bin/edu.js post /api/some/endpoint   # 从 stdin 读取请求体
```

选项：`--cookies <file>`、`--pretty`（默认，输出精炼的中文摘要）、`--raw`（输出完整 JSON；对 `get`/`post`/`raw` 则为原样响应体，不解析 JSON）。

通过 `npm link` 安装为全局 `edu` 命令后，可省略 `node bin/edu.js`。注意：

- `npm link` 是符号链接，**移动或删除本仓库目录后命令会失效**。
- 卸载：`npm unlink -g educoder-helper`。
- 若提示找不到 `edu`，请确认 npm 全局 bin 目录（`npm prefix -g`）已加入 PATH。

## 库用法

```js
import { EduClient } from './edu/index.js';

const edu = new EduClient();                 // 或传入 { cookiesPath, host }

// 用户 & 课程
await edu.getUserInfo();
await edu.getCourses();                      // 默认取当前登录用户
await edu.getHomeworks(113636);              // type 默认为 4（实训作业）

// 实训 & 关卡
await edu.getChallenges('gu8nbv56');         // 实训的关卡列表（shixun identifier）
await edu.getMyChallenges('26svp3tnax');     // 我的关卡实例列表（myshixun identifier）
await edu.getTask('sfpqjg3biyn6');          // 单关详情，task_pass 为任务描述 Markdown
await edu.getFileContent('sfpqjg3biyn6', 'src/shell1.sh'); // 仓库文件内容（已 Base64 解码）
await edu.getWorkReport(296406531);          // 作业报告（stage_list）
await edu.enterShixun('gu8nbv56');          // 进入实训 → { game_identifier }

// 导出
import { exportChallenge, exportShixun, exportCourse } from './edu/index.js';
await exportChallenge(edu, gameId);   // 单关，dir 默认为 challenge.subject
await exportShixun(edu, myshixunId);  // 整个实训，dir 默认为 shixun.name
await exportCourse(edu, courseId);    // 整个课程的所有实训作业，dir 默认为 course.name

// 直接访问任意 API
await edu.get('/api/courses/113636/homework_commons.json?type=4');
await edu.post('/api/.../endpoint', { field: 'value' });
const { status, body } = await edu.request('GET', '/api/...', { raw: true });
```

`get/post/put/delete` 在非 2xx 响应时抛出异常，异常对象携带 `err.status` 和 `err.data`。
传入 `{ raw: true }` 可获得 `{ status, redirected, url, body }` 原始结果，不抛出异常。

## 目录结构

```
edu/
  bin/edu.js     CLI 入口
  src/client.js  EduClient（认证、签名、便捷方法）
  src/sign.js    签名算法及 ak/sk 常量
  src/cookies.js cookies.txt 加载与路径解析
  src/exporter.js 导出实训代码与任务描述
  index.js       库导出
```

## 免责声明

本项目仅供学习与研究使用，用于个人备份自己账号下的数据，请遵守 `educoder.net` 的服务条款及相关法律法规。

- 使用本工具产生的一切后果由使用者自行承担，作者不承担任何责任。
- 请勿用于未经授权的访问、批量抓取、绕过平台限制或任何侵犯他人权益的用途。
- Cookie 即你的登录凭证，请妥善保管 `cookies.txt`，切勿泄露或提交到版本库。
- 本项目与 EduCoder 官方无任何关联。
