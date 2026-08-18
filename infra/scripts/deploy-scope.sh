#!/usr/bin/env bash
#
# Map a release tag to the deploy scope for `.github/workflows/deploy-production.yml`.
#
# Grammar:  production-YYYY.MM.DD-N[-SELECTOR]
#
#   (none)        Compute + SPA           the standard release set
#   all           every differing stack + SPA
#   web           SPA content only, no CDK deploy
#   <StackName>   Explorer-production-<StackName>, --exclusively, no SPA
#
# Prints the three `key=value` lines the workflow appends verbatim to
# $GITHUB_OUTPUT. Exits 1 on a tag that does not match the grammar.
#
# Emitting the FINAL format, not fields for the caller to re-split, is
# deliberate: the first version printed TAB-separated fields and the workflow
# re-read them with `IFS=$'\t' read`, which silently collapsed the empty ones
# (TAB is IFS whitespace) — `-web` came back with the stack set to `true`. The
# self-check passed anyway, because it tested the script's output and not what
# CI actually consumed. One format, one writer, no seam to drift.
#
# `--self-check` runs the same function CI runs:
#
#   ./infra/scripts/deploy-scope.sh --self-check
#
set -euo pipefail

scope_for_tag() {
  local tag="$1" sel stacks excl web
  # Anchored on purpose. The trigger is `production-*`, so before this a stray
  # `production-experiment` deployed the full release set.
  if [[ ! "$tag" =~ ^production-[0-9]{4}\.[0-9]{2}\.[0-9]{2}-[0-9]+(-(.+))?$ ]]; then
    return 1
  fi
  sel="${BASH_REMATCH[2]:-}"
  case "$sel" in
    '') stacks='Explorer-production-Compute' excl='--exclusively' web=true ;;
    all) stacks='--all' excl='' web=true ;;
    web) stacks='' excl='' web=true ;;
    # Verbatim, so the selector must carry the stack's own case. An unknown
    # name is not silent — cdk fails on it before anything deploys.
    *) stacks="Explorer-production-$sel" excl='--exclusively' web=false ;;
  esac
  # Note the `all)` arm leaves `excl` empty: `--all` and `--exclusively`
  # contradict each other and cdk rejects the pair.
  printf 'stacks=%s\nexclusively=%s\nweb=%s\n' "$stacks" "$excl" "$web"
}

self_check() {
  local fails=0 got want
  check() {
    got="$(scope_for_tag "$1" 2>/dev/null | tr '\n' ' ')" || true
    [ -n "$got" ] || got='REJECT '
    want="$2 "
    if [ "$got" != "$want" ]; then
      printf 'FAIL  %-38s got [%s] want [%s]\n' "$1" "${got% }" "${want% }"
      fails=1
    fi
  }

  # The one that matters: a plain release tag must stay Compute-only. If this
  # ever reads `--all`, every tag silently ships parked drift.
  check 'production-2026.08.18-1' \
    'stacks=Explorer-production-Compute exclusively=--exclusively web=true'
  check 'production-2026.08.18-12' \
    'stacks=Explorer-production-Compute exclusively=--exclusively web=true'
  # Empty fields are the ones that broke before — pin both of them.
  check 'production-2026.08.18-1-all' 'stacks=--all exclusively= web=true'
  check 'production-2026.08.18-1-web' 'stacks= exclusively= web=true'
  check 'production-2026.08.18-1-CloudWatch' \
    'stacks=Explorer-production-CloudWatch exclusively=--exclusively web=false'
  check 'production-2026.08.18-1-CloudflareBootstrap' \
    'stacks=Explorer-production-CloudflareBootstrap exclusively=--exclusively web=false'

  # Rejected shapes — each one deployed the release set before the anchor.
  check 'production-experiment' 'REJECT'
  check 'production-2026.08.18' 'REJECT'
  check 'production-26.8.18-1' 'REJECT'
  check 'staging-2026.08.18-1' 'REJECT'
  check 'xproduction-2026.08.18-1' 'REJECT'

  if [ "$fails" -eq 0 ]; then echo 'deploy-scope self-check OK'; else exit 1; fi
}

if [ "${1:-}" = '--self-check' ]; then
  self_check
else
  scope_for_tag "${1:?usage: deploy-scope.sh <tag> | --self-check}"
fi
