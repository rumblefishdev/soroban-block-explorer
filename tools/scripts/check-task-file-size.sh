#!/usr/bin/env sh
# Refuse a commit that grows a lore task's main file past the size limit set in
# lore/1-tasks/CLAUDE.md ("Task Size"). The main file is the single-file task
# (lore/1-tasks/<status>/NNNN_*.md) or a task directory's README.md; notes/ are
# free to grow. It is a ratchet: a file already over the limit may stay as it
# is or shrink, it may not grow, and a new file may not start over the limit.
#
# The staged copy is compared with the HEAD copy. A task moved between status
# directories (git mv) is compared with its HEAD copy at the old path, so
# promoting or archiving a long task does not count as growth.
set -eu

limit=150
tab="$(printf '\t')"
failed=0

count_lines() {
  if git cat-file -e "$1" 2>/dev/null; then
    git show "$1" | wc -l | tr -d ' '
  else
    echo 0
  fi
}

changes="$(git diff --cached --name-status -M --diff-filter=AMR)"

while IFS="$tab" read -r state old new; do
  # A and M lines carry one path, R lines carry the old and the new one.
  [ -n "$new" ] || new="$old"

  case "$new" in
    lore/1-tasks/*) ;;
    *) continue ;;
  esac
  rest="${new#lore/1-tasks/}"
  case "${rest%%/*}" in
    backlog | active | blocked | archive) ;;
    *) continue ;;
  esac
  case "${rest#*/}" in
    */*/*) continue ;;
    [0-9][0-9][0-9][0-9]_*/README.md) ;;
    */*) continue ;;
    [0-9][0-9][0-9][0-9]_*.md) ;;
    *) continue ;;
  esac

  staged="$(count_lines ":$new")"
  if [ "$state" = A ]; then
    before=0
  else
    before="$(count_lines "HEAD:$old")"
  fi

  if [ "$staged" -gt "$limit" ] && [ "$staged" -gt "$before" ]; then
    task="${new%/README.md}"
    task="${task%.md}"
    echo "$new: $before → $staged lines (limit $limit)." >&2
    echo "  Move detail into $task/notes/ (convert a single-file task to a directory first); keep the main file to the current state." >&2
    failed=1
  fi
done <<EOF
$changes
EOF

exit "$failed"
