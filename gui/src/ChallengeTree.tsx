// 课程 → 实训 → 关卡的勾选树，导出页和实验报告页共用。
// 只负责「选了哪些关卡」：展开状态自己管，选中集合由调用方持有，
// 这样两边都能把选择结果直接喂给后端的 SelectedHomework[]。
import { useEffect, useRef, useState } from "react";
import { ChevronDown, ChevronRight } from "lucide-react";

import type { ReportTree, SelectedHomework, TreeHomework } from "./types";
import { Badge } from "./ui";

/** 一棵树里所有可选关卡的 gameId。 */
export function allGameIds(tree: ReportTree): Set<string> {
  return new Set(tree.homeworks.flatMap((h) => h.challenges.map((c) => c.gameId)));
}

/** 把勾选结果整理成后端要的形状，丢掉一个都没选的实训。 */
export function selectedHomeworks(tree: ReportTree, picked: Set<string>): SelectedHomework[] {
  return tree.homeworks
    .map((h) => ({
      name: h.name,
      total: h.challenges.length,
      challenges: h.challenges.filter((c) => picked.has(c.gameId)),
    }))
    .filter((h) => h.challenges.length > 0);
}

export function countPicked(tree: ReportTree, picked: Set<string>): number {
  return selectedHomeworks(tree, picked).reduce((n, h) => n + h.challenges.length, 0);
}

export function ChallengeTree({
  tree,
  picked,
  onChange,
  disabled = false,
}: {
  tree: ReportTree;
  picked: Set<string>;
  onChange: (next: Set<string>) => void;
  disabled?: boolean;
}) {
  const [expanded, setExpanded] = useState<Set<string>>(new Set());

  function toggleOne(gameId: string) {
    const next = new Set(picked);
    if (next.has(gameId)) next.delete(gameId);
    else next.add(gameId);
    onChange(next);
  }

  function toggleAll(hw: TreeHomework, on: boolean) {
    const next = new Set(picked);
    for (const c of hw.challenges) {
      if (on) next.add(c.gameId);
      else next.delete(c.gameId);
    }
    onChange(next);
  }

  function toggleExpanded(name: string) {
    setExpanded((prev) => {
      const next = new Set(prev);
      if (next.has(name)) next.delete(name);
      else next.add(name);
      return next;
    });
  }

  const total = countPicked(tree, picked);
  const groups = selectedHomeworks(tree, picked).length;

  return (
    <>
      <div className="tree-toolbar">
        <button
          className="btn btn-sm"
          onClick={() => onChange(allGameIds(tree))}
          disabled={disabled}
        >
          全选
        </button>
        <button className="btn btn-sm" onClick={() => onChange(new Set())} disabled={disabled}>
          全不选
        </button>
        <button
          className="btn btn-sm"
          onClick={() => setExpanded(new Set(tree.homeworks.map((h) => h.name)))}
          disabled={disabled}
        >
          展开全部
        </button>
        <button className="btn btn-sm" onClick={() => setExpanded(new Set())} disabled={disabled}>
          收起全部
        </button>
        <span className="tree-count muted small">
          已选 {total} 关 / {groups} 个实训
        </span>
      </div>

      <ul className="tree">
        {tree.homeworks.map((hw) => (
          <HomeworkNode
            key={hw.name}
            hw={hw}
            picked={picked}
            open={expanded.has(hw.name)}
            disabled={disabled}
            onToggleOpen={() => toggleExpanded(hw.name)}
            onToggleAll={(on) => toggleAll(hw, on)}
            onToggleOne={toggleOne}
          />
        ))}
      </ul>
    </>
  );
}

function HomeworkNode({
  hw,
  picked,
  open,
  disabled,
  onToggleOpen,
  onToggleAll,
  onToggleOne,
}: {
  hw: TreeHomework;
  picked: Set<string>;
  open: boolean;
  disabled: boolean;
  onToggleOpen: () => void;
  onToggleAll: (on: boolean) => void;
  onToggleOne: (gameId: string) => void;
}) {
  const boxRef = useRef<HTMLInputElement>(null);
  const mine = hw.challenges.filter((c) => picked.has(c.gameId)).length;
  const all = hw.challenges.length;

  // 部分选中要显示成 indeterminate，这个状态只能通过 DOM 属性设置。
  useEffect(() => {
    if (boxRef.current) boxRef.current.indeterminate = mine > 0 && mine < all;
  }, [mine, all]);

  return (
    <li className="tree-node">
      <div className="tree-row">
        <button
          type="button"
          className="tree-toggle"
          onClick={onToggleOpen}
          disabled={all === 0}
          aria-label={open ? "收起" : "展开"}
        >
          {all === 0 ? null : open ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
        </button>
        <label className="tree-label">
          <input
            ref={boxRef}
            type="checkbox"
            checked={all > 0 && mine === all}
            onChange={(e) => onToggleAll(e.target.checked)}
            disabled={disabled || all === 0}
          />
          <span className="tree-name">{hw.name}</span>
        </label>
        {hw.skipped ? (
          <Badge tone="warn">{hw.skipped}</Badge>
        ) : (
          <span className={`tree-count muted small${mine > 0 ? " is-on" : ""}`}>
            {mine}/{all}
          </span>
        )}
      </div>

      {open && all > 0 ? (
        <ul className="tree-children">
          {hw.challenges.map((c) => (
            <li key={c.gameId}>
              <label className="tree-label">
                <input
                  type="checkbox"
                  checked={picked.has(c.gameId)}
                  onChange={() => onToggleOne(c.gameId)}
                  disabled={disabled}
                />
                <span className="pos">{String(c.position ?? "??").padStart(2, "0")}</span>
                <span className="tree-name">{c.name}</span>
              </label>
            </li>
          ))}
        </ul>
      ) : null}
    </li>
  );
}
