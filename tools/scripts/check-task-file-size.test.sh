#!/usr/bin/env sh
# Exercise check-task-file-size.sh against throwaway git repositories.
# Usage: sh tools/scripts/check-task-file-size.test.sh [scratch-dir]
# Each case gets its own repo under a fresh temp dir (in scratch-dir when
# given, else under $TMPDIR); the dir is left behind for inspection.
set -eu

script="$(cd "$(dirname "$0")" && pwd)/check-task-file-size.sh"
root="$(mktemp -d "${1:-${TMPDIR:-/tmp}}/task-file-size.XXXXXX")"
failures=0
n=0

lines() { # lines <count> <path>: write a file of <count> lines
  mkdir -p "$(dirname "$2")"
  i=0
  : >"$2"
  while [ "$i" -lt "$1" ]; do
    i=$((i + 1))
    echo "line $i" >>"$2"
  done
}

new_repo() {
  n=$((n + 1))
  repo="$root/case$n"
  mkdir -p "$repo"
  git -C "$repo" init -q
  git -C "$repo" config user.email test@example.invalid
  git -C "$repo" config user.name test
  git -C "$repo" commit -q --allow-empty -m init
}

commit_all() {
  git -C "$repo" add -A
  git -C "$repo" commit -q -m setup
}

expect() { # expect pass|fail <description>
  if (cd "$repo" && sh "$script") >"$repo.out" 2>&1; then got=pass; else got=fail; fi
  if [ "$got" = "$1" ]; then
    echo "ok   $2 ($got)"
  else
    echo "FAIL $2: expected $1, got $got"
    sed 's/^/     /' "$repo.out"
    failures=$((failures + 1))
  fi
}

task=lore/1-tasks/active/0001_FEATURE_example.md

new_repo
lines 200 "$repo/$task"
git -C "$repo" add -A
expect fail "new 200-line task file"

new_repo
lines 160 "$repo/$task"
commit_all
lines 170 "$repo/$task"
git -C "$repo" add -A
expect fail "160 -> 170 lines"

new_repo
lines 170 "$repo/$task"
commit_all
lines 160 "$repo/$task"
git -C "$repo" add -A
expect pass "170 -> 160 lines"

new_repo
lines 100 "$repo/$task"
commit_all
lines 140 "$repo/$task"
git -C "$repo" add -A
expect pass "100 -> 140 lines"

new_repo
lines 500 "$repo/lore/1-tasks/active/0002_FEATURE_dir/notes/R-research.md"
git -C "$repo" add -A
expect pass "notes/ file of 500 lines"

new_repo
lines 200 "$repo/lore/1-tasks/active/0003_FEATURE_dir/README.md"
git -C "$repo" add -A
expect fail "new 200-line directory README.md"

new_repo
lines 300 "$repo/lore/1-tasks/backlog/0004_FEATURE_big.md"
commit_all
mkdir -p "$repo/lore/1-tasks/active"
git -C "$repo" mv lore/1-tasks/backlog/0004_FEATURE_big.md lore/1-tasks/active/
expect pass "git mv of an unchanged 300-line task backlog -> active"

new_repo
lines 300 "$repo/lore/1-tasks/backlog/0005_FEATURE_big.md"
commit_all
mkdir -p "$repo/lore/1-tasks/active"
git -C "$repo" mv lore/1-tasks/backlog/0005_FEATURE_big.md lore/1-tasks/active/
echo "one more line" >>"$repo/lore/1-tasks/active/0005_FEATURE_big.md"
git -C "$repo" add -A
expect fail "git mv of a 300-line task that also grows"

new_repo
lines 300 "$repo/lore/1-tasks/active/0006_FEATURE_long.md"
commit_all
mkdir -p "$repo/lore/1-tasks/active/0006_FEATURE_long"
git -C "$repo" mv lore/1-tasks/active/0006_FEATURE_long.md lore/1-tasks/active/0006_FEATURE_long/README.md
printf 'rewritten %s\n' $(seq 1 160) >"$repo/lore/1-tasks/active/0006_FEATURE_long/README.md"
lines 200 "$repo/lore/1-tasks/active/0006_FEATURE_long/notes/S-history.md"
git -C "$repo" add -A
expect pass "300-line task turned into a directory with a rewritten 160-line README"

new_repo
lines 300 "$repo/lore/1-tasks/active/0007_FEATURE_long.md"
commit_all
mkdir -p "$repo/lore/1-tasks/active/0007_FEATURE_long"
git -C "$repo" mv lore/1-tasks/active/0007_FEATURE_long.md lore/1-tasks/active/0007_FEATURE_long/README.md
printf 'rewritten %s\n' $(seq 1 310) >"$repo/lore/1-tasks/active/0007_FEATURE_long/README.md"
git -C "$repo" add -A
expect fail "task turned into a directory whose README grows past its old file"

new_repo
lines 300 "$repo/$task"
commit_all
git -C "$repo" checkout -q -b side
lines 320 "$repo/$task"
commit_all
git -C "$repo" checkout -q -
echo "unrelated" >"$repo/other.txt"
commit_all
git -C "$repo" merge -q --no-commit --no-ff side
expect pass "merge brings in a 300 -> 320 growth made on the other branch"

new_repo
lines 300 "$repo/$task"
commit_all
git -C "$repo" checkout -q -b side
lines 320 "$repo/$task"
commit_all
git -C "$repo" checkout -q -
git -C "$repo" merge -q --no-commit --no-ff side
lines 330 "$repo/$task"
git -C "$repo" add -A
expect fail "merge that grows the task past both parents"

echo "repos kept in $root"
[ "$failures" -eq 0 ] || { echo "$failures case(s) failed"; exit 1; }
echo "all cases passed"
