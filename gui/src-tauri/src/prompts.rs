//! The prompt set that turns exported challenges into a lab report.
//!
//! Kept apart from `report.rs` (which only orchestrates) because this is the
//! part that gets tuned. Three builders, one per kind of call:
//!   * [`section`]    — one 实训 → one `2.x` section, the bulk of the report
//!   * [`introduction`] — chapter 1, written after the sections are known
//!   * [`conclusion`]   — chapter 3, given a digest of what was actually done
//!
//! Two conventions the rest of the app depends on:
//!   * the model emits `待插入截图` / `待补充运行结果` markers wherever a human
//!     has to paste something — `report::count_placeholders` counts them;
//!   * the model returns raw markdown, never fenced in ```markdown.

/// Per-challenge caps. A single 实训 can carry a dozen challenges, and some
/// task descriptions run thousands of characters; without a cap one request can
/// blow past a small model's context.
const MAX_TASK_CHARS: usize = 6000;
const MAX_CODE_CHARS: usize = 8000;

fn clip(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let head: String = s.chars().take(max).collect();
    format!("{head}\n…（内容过长，此处截断）")
}

/// One challenge's raw material, as handed to the model.
pub struct ChallengeInput {
    pub number: String,
    pub name: String,
    pub task: String,
    pub files: Vec<(String, String)>,
}

impl ChallengeInput {
    fn render(&self) -> String {
        let mut out = format!(
            "#### 关卡 {} {}\n\n【关卡任务描述（来自平台）】\n{}\n",
            self.number,
            self.name,
            clip(&self.task, MAX_TASK_CHARS)
        );
        if self.files.is_empty() {
            out.push_str("\n【学生提交的代码】（未取到代码文件）\n");
        }
        for (path, code) in &self.files {
            out.push_str(&format!(
                "\n【学生提交的代码 · {}】\n```\n{}\n```\n",
                path,
                clip(code, MAX_CODE_CHARS)
            ));
        }
        out
    }
}

/// Shared voice and formatting rules. Everything the model must respect on
/// every call lives here so the three builders stay short.
pub const SYSTEM: &str = r#"你是一名正在撰写课程实践报告的大学生，需要根据平台上的关卡任务描述和自己提交的代码，写出这份报告的正文。

写作要求：
1. 用中文，第一人称视角但避免频繁出现"我"，语气客观、朴实、简洁。这是课程作业报告，不是宣传稿：不要夸张修辞，不要"极大提升""充分体现"这类空话，不要给自己戴高帽。
2. 只输出 Markdown 正文。不要用 ```markdown 把整个回答包起来，不要写"好的""以下是"这类开场白，不要在结尾添加与正文无关的说明。
3. 代码用带语言标注的围栏代码块（```sql、```java、```cpp 等）。
4. 详略规则（重要）：
   - 简单的解题代码（排版后约 4 行以内）直接粘贴即可，不要逐行解释，不要为它配图。
   - 复杂的代码要讲清楚思路和关键点：为什么这样写、难点在哪、必要处加注释。分析文字控制在半页以内。
   - 不要复述任务描述原文，要用自己的话概括任务要求。
5. 需要人工补充材料的地方，使用下面两种占位符，不要自己编造运行结果或截图内容：
   - 需要截图时：
     > 🖼️ **待插入截图**：（一句话说明这张图应当展示什么）
     >
     > ![图 X.Y 图题](images/建议文件名.png)
   - 需要粘贴实际输出、查询结果表格时：
     > 📋 **待补充运行结果**：（一句话说明这里应当粘贴什么）
   截图数量克制：一个关卡最多 4 张，直接粘贴简单代码的关卡不配图。
6. 绝不编造：任务描述或代码里没有的功能、数据、性能数字一律不要写。代码没取到就说明情况，不要虚构代码。"#;

/// One 实训 → one `2.x` section.
#[allow(clippy::too_many_arguments)]
pub fn section(
    course_name: &str,
    section_no: usize,
    homework_name: &str,
    total_challenges: usize,
    challenges: &[ChallengeInput],
    task_book: &str,
    requirements: &str,
) -> String {
    let mut body = String::new();
    for c in challenges {
        body.push_str(&c.render());
        body.push('\n');
    }

    let selected = challenges.len();
    let coverage = if total_challenges > 0 && selected < total_challenges {
        format!(
            "该实训共 {total_challenges} 个关卡，本报告选取其中 {selected} 个有代表性的关卡展开。"
        )
    } else {
        format!("该实训的 {selected} 个关卡均在本报告中展开。")
    };

    let special = special_requirements(homework_name);

    format!(
        r#"这是《{course_name}》课程实践报告的第 2 章「任务实施过程与分析」中的一节，请只写这一节。

本节对应的实训任务：{homework_name}
本节在报告中的编号：2.{section_no}
{coverage}

请严格按下面的结构输出：

### 2.{section_no} {homework_name}

（先用一段话——原则上 5 行以内——总体概括本实训任务的内容与要求，最后一句说明本实训任务已完成哪些关卡。不要分点，就是一段连贯的文字。）

#### 2.{section_no}.1 （第一个关卡的名称）

（阐述该关卡的完成过程，按系统提示中的详略规则处理代码与配图。）

#### 2.{section_no}.2 （第二个关卡的名称）

……以此类推，为下面给出的每一个关卡各写一个四级小节，顺序与下面给出的顺序一致。小节标题用"2.{section_no}.序号 关卡名称"。
{special}
=== 课程任务书（供你理解本实训的背景与要求，不要整段照抄）===
{task_book}

=== 实验/报告要求（供你把握详略与体例）===
{requirements}

=== 本节需要写的关卡材料 ===
{body}"#,
        task_book = clip(task_book, 12000),
        requirements = clip(requirements, 8000),
    )
}

/// Two 实训 carry extra, non-negotiable subsections imposed by the engineering
/// accreditation requirements in the template.
fn special_requirements(homework_name: &str) -> String {
    if homework_name.contains("设计与实现") || homework_name.contains("数据库设计") {
        return r#"
【本节的额外要求】本实训是"数据库设计与实现"，报告要求中明确规定不得略过以下两个小节，请在本节最后追加它们（编号接在关卡小节之后）：
  - "制约因素分析与设计"：结合本实训的具体设计内容（业务流程、数据结构与关联、数据约束、权限设计等），阐述方案设计中如何考虑了社会、健康、安全、法律、文化及环境等制约因素。可逐条归纳，篇幅一段话到一页。
  - "工程师责任及其分析"：分析本任务的解决过程与解决方案同社会、健康、安全、法律以及文化等因素之间的相互影响，阐述对工程师应承担的社会责任的理解。篇幅一段话到一页。
本节的过程阐述与配图不受"一个关卡最多 4 张图"的限制，数据库结构展示可以适当多配图。
"#
        .to_string();
    }
    if homework_name.contains("应用开发") || homework_name.contains("JAVA") || homework_name.contains("Java") {
        return "\n【本节的额外要求】本实训是\"数据库应用开发\"，过程阐述与流程图等配图不受"
            .to_string()
            + "\"一个关卡最多 4 张图\"的限制，可依据实际情况展开。\n";
    }
    String::new()
}

/// Chapter 1, written once the section list is known.
pub fn introduction(course_name: &str, homework_names: &[String], task_book: &str) -> String {
    let list = homework_names
        .iter()
        .enumerate()
        .map(|(i, n)| format!("{}. {n}", i + 1))
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        r#"这是《{course_name}》课程实践报告的第 1 章，请只写这一章。

要求：简要陈述本实践课程的总体任务要求及其任务分解情况。篇幅不超过 1 页（约 500~700 字），不要写成流水账，也不要罗列格式说明。

请按下面的结构输出：

## 1 课程任务概述

（正文。可以先交代课程的定位与实践平台、实验环境，再说明总体任务由哪些实训任务构成——可用有序列表分模块列出——最后一句交代本报告将重点阐述哪些内容。）

=== 本次报告涉及的实训任务 ===
{list}

=== 课程任务书 ===
{task_book}"#,
        task_book = clip(task_book, 14000),
    )
}

/// Chapter 3, given a digest of what the earlier calls actually produced.
pub fn conclusion(course_name: &str, digest: &str, requirements: &str) -> String {
    format!(
        r#"这是《{course_name}》课程实践报告的第 3 章，请只写这一章。

报告要求本章分为三个部分，篇幅控制在一页以内（约 700~900 字）：
1. 概述、总结本次课程实践的总体任务及其完成情况；
2. 归纳本次实践的主要工作——按能力模块归纳（如 SQL 语言的综合应用、存储过程与事务、并发控制、数据库设计、应用开发、DBMS 内核实现等），不要照抄任务书，要写自己实际做了什么；
3. 心得体会，以及本次课程实践有待改进和完善的地方。第三部分要具体，结合实际遇到的困难来写，不要空泛地喊口号。

请按下面的结构输出：

## 3 课程总结

### 3.1 总体任务完成情况

### 3.2 主要工作内容

### 3.3 心得体会与改进方向

=== 本次实践实际完成的内容（据此归纳，不要超出这个范围）===
{digest}

=== 实验/报告要求 ===
{requirements}"#,
        requirements = clip(requirements, 6000),
    )
}

/// The 绪言 paragraph that opens chapter 2, per the template.
pub fn chapter2_preamble(course_name: &str, homework_names: &[String]) -> String {
    let list = homework_names.join("、");
    format!(
        r#"这是《{course_name}》课程实践报告第 2 章的绪言段落，请只写这一段，不要写标题，不要分点。

模板给出的写法是："本次实践课程在头歌平台进行，实践任务均在平台上提交代码，所有完成的任务、关卡均通过了自动测评。本次实践最终完成了课程平台中的第 X、Y~Z 实训任务，下面将重点针对其中的 XXX 任务阐述其完成过程中的具体工作。"

请保留这个句式和语气，把其中的任务范围替换为下面实际报告涉及的实训任务。一段话即可，控制在 5 行以内。

=== 本报告第 2 章将阐述的实训任务（按顺序）===
{list}"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clip_keeps_short_text_intact() {
        assert_eq!(clip("abc", 10), "abc");
        assert!(clip(&"啊".repeat(100), 10).contains("截断"));
        // Counts chars, not bytes: 10 CJK chars must survive a 10-char cap.
        assert_eq!(clip(&"啊".repeat(10), 10), "啊".repeat(10));
    }

    #[test]
    fn database_design_section_carries_the_accreditation_subsections() {
        let s = special_requirements("MySQL-数据库设计与实现");
        assert!(s.contains("制约因素分析与设计"));
        assert!(s.contains("工程师责任及其分析"));
        assert!(special_requirements("视图").is_empty());
    }

    #[test]
    fn java_section_lifts_the_figure_cap() {
        assert!(special_requirements("数据库应用开发(JAVA篇)").contains("不受"));
    }

    #[test]
    fn section_prompt_states_coverage_both_ways() {
        let c = vec![ChallengeInput {
            number: "1".into(),
            name: "第1关".into(),
            task: "t".into(),
            files: vec![],
        }];
        assert!(section("课", 3, "视图", 5, &c, "", "").contains("选取其中 1 个"));
        assert!(section("课", 3, "视图", 1, &c, "", "").contains("均在本报告中展开"));
    }

    #[test]
    fn challenge_render_marks_missing_code() {
        let c = ChallengeInput {
            number: "2".into(),
            name: "第2关".into(),
            task: "描述".into(),
            files: vec![],
        };
        assert!(c.render().contains("未取到代码文件"));
    }
}
