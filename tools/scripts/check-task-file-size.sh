#!/usr/bin/env sh
# Refuse a commit that grows a lore task's main file past the size limit set in
# lore/1-tasks/CLAUDE.md ("Task Size"). The main file is the single-file task
# (lore/1-tasks/<status>/NNNN_*.md) or a task directory's README.md; notes/ are
# free to grow. It is a ratchet: a file already over the limit may stay as it
# is or shrink, it may not grow, and a new file may not start over the limit.
#
# The staged copy is compared with the task's main file in HEAD, found by its
# task id wherever it was: a task moved between status directories, or turned
# from a single file into a directory, is measured against what it replaced,
# so neither counts as growth. Git's own rename pairing is not enough: it
# needs the two files to be at least 50% alike, and a README rewritten while
# the detail moves to notes/ is not.
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
  # Two tasks can share an id after a merge; the larger of them counts.
  before=0
  id="$(printf '%s' "${rest#*/}" | cut -c1-4)"
  for path in $(git ls-tree -r --name-only HEAD lore/1-tasks |
    grep -E "^lore/1-tasks/(backlog|active|blocked|archive)/${id}_[^/]*(\.md|/README\.md)\$"); do
    n="$(count_lines "HEAD:$path")"
    [ "$n" -gt "$before" ] && before="$n"
  done

  if [ "$staged" -gt "$limit" ] && [ "$staged" -gt "$before" ]; then
    echo "$new: $before → $staged lines (limit $limit)." >&2
    case "$new" in
      */README.md) echo "  Move detail into ${new%/README.md}/notes/; keep the README to the current state." >&2 ;;
      *) echo "  Turn the task into a directory (${new%.md}/README.md) and move detail into its notes/; keep the README to the current state." >&2 ;;
    esac
    failed=1
  fi
done <<EOF
$changes
EOF

exit "$failed"
