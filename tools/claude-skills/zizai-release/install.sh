#!/usr/bin/env bash
# 在本机把仓库内 SKILL.md 复制到 ~/.agents/skills/zizai-release 让 Claude 能发现它。
# 源文件是 source of truth——只要重跑这个脚本就同步。
#
# 用法：
#   bash tools/claude-skills/zizai-release/install.sh
#
# 需要的目录：
#   ~/.agents/skills/zizai-release/  (Skill 系统扫描路径)
set -euo pipefail

REPO_ROOT="$(git rev-parse --show-toplevel)"
SRC="$REPO_ROOT/tools/claude-skills/zizai-release/SKILL.md"
DST_DIR="${HOME}/.agents/skills/zizai-release"
DST="${DST_DIR}/SKILL.md"

if [[ ! -f "$SRC" ]]; then
    echo "找不到源文件 $SRC" >&2
    exit 1
fi

mkdir -p "$DST_DIR"

# 优先尝试创建 symlink，失败退化为 copy。
if command -v ln >/dev/null 2>&1 && ln -sf "$SRC" "$DST" 2>/dev/null; then
    echo "已 symlink $DST -> $SRC"
elif cp "$SRC" "$DST"; then
    echo "已 copy $DST（无法创建软链接）"
else
    echo "复制失败" >&2
    exit 1
fi

# 可选：在 ~/.claude/skills/ 下也放一份（部分 harness 扫描这条路径）
CLAUDE_DIR="${HOME}/.claude/skills/zizai-release"
if [[ -d "${HOME}/.claude/skills" || -d "${HOME}/.claude" ]]; then
    mkdir -p "$CLAUDE_DIR"
    if ln -sf "$SRC" "${CLAUDE_DIR}/SKILL.md" 2>/dev/null; then
        echo "已 symlink ${CLAUDE_DIR}/SKILL.md -> $SRC"
    elif cp "$SRC" "${CLAUDE_DIR}/SKILL.md"; then
        echo "已 copy ${CLAUDE_DIR}/SKILL.md"
    fi
fi

echo "完成。下一轮 Claude 会话可识别 zizai-release skill。"