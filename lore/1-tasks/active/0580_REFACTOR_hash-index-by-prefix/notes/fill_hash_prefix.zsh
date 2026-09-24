# Task 0580: fill transaction_hash_index_staging_prefix from the old index,
# slice by slice, gating each slice. Writes production, so the operator runs
# it, from the shell where `chw` / `chq` are defined (shell functions, hence
# `source`):
#
#   source <path>/fill_hash_prefix.zsh 100 101 102
#
# Items:
#   P        whole partition P (ledgers P*500000 .. P*500000+499999)
#   P:A      resume partition P at slice A (a multiple of 50,000 inside P)
#   A-B      explicit ledger range [A, B) — the head's partition and the tail
#            after the indexer is paused; B is exclusive
#
# Per slice [a, a+50000) (clipped to B in range mode): fill_hash_prefix.sql
# through chw, then the gate through chq — distinct (prefix, ledger) of the old
# index, computed from its full hash, against the staging copy, per quarter
# slice so no query nears the memory cap. Stops at the first error or
# mismatch, naming the slice. Re-running a slice is safe (the staging
# ReplacingMergeTree collapses repeated rows). Stops before any partition when
# free disk is under 120 GiB.
() {
  local N=${1:A:h} T=transaction_hash_index S=transaction_hash_index_staging_prefix item P A B a b from to out free g q lo hi side tbl key old new
  shift
  for item in "$@"; do
    if [[ $item == <->-<-> ]]; then
      from=${item%%-*}; to=${item#*-}
    elif [[ $item == <->:<-> ]]; then
      P=${item%%:*}; A=${item#*:}
      if (( A % 50000 || A / 500000 != P )); then
        print -r -- "STOP: resume point $A is not a 50,000-ledger slice of partition $P"
        return 1
      fi
      from=$A; to=$((P * 500000 + 500000))
    elif [[ $item == <-> ]]; then
      from=$((item * 500000)); to=$((item * 500000 + 500000))
    else
      print -r -- "STOP: '$item' is not P, P:A or A-B"
      return 1
    fi
    free=$(chq "SELECT intDiv(free_space, 1073741824) FROM system.disks WHERE name = 'default'")
    if [[ $free != <-> ]] || (( free < 120 )); then
      print -r -- "STOP before $item: free disk reads '$free' GiB"
      return 1
    fi
    for a in $(seq -f '%.0f' $from 50000 $((to - 1))); do
      b=$((a + 50000)); (( b > to )) && b=$to
      # A successful INSERT prints nothing. Anything else — a server error, or
      # a transport error from curl on stderr — stops the loop.
      out=$(chw "$(sed -e "s/{A}/$a/g" -e "s/{B}/$b/g" "$N/fill_hash_prefix.sql")" 2>&1)
      if (( $? )) || [[ -n $out ]]; then
        print -r -- "FAILED fill [$a, $b): $out"
        return 1
      fi
      # stdout only: a shell warning on stderr must not read as a count; a
      # transport error leaves it empty and fails below.
      old=0; new=0
      for q in 0 1 2 3; do
        lo=$((a + (b - a) * q / 4)); hi=$((a + (b - a) * (q + 1) / 4))
        for side in old new; do
          if [[ $side == old ]]; then tbl=$T; key='reinterpretAsUInt64(substring(hash, 1, 8))'; else tbl=$S; key=hash_prefix; fi
          g=$(chq "$(sed -e "s/{TBL}/$tbl/g" -e "s/{K}/$key/g" -e "s/{LO}/$lo/g" -e "s/{HI}/$hi/g" "$N/gate_hash_prefix.sql")" 2>/dev/null)
          if [[ $g != <-> ]]; then
            print -r -- "FAILED gate [$lo, $hi) $side: '$g'"
            return 1
          fi
          if [[ $side == old ]]; then old=$((old + g)); else new=$((new + g)); fi
        done
      done
      if (( old != new )); then
        print -r -- "FAILED gate [$a, $b): old keys $old, new keys $new"
        return 1
      fi
    done
    print -r -- "ok $item $(date +%H:%M) free ${free} GiB"
  done
} "${(%):-%x}" "$@"
