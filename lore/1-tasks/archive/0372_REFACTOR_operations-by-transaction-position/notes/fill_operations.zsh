# Task 0372: fill transaction_operations and pool_operation_amounts from the
# surrogate-keyed tables, slice by slice, gating each slice. Runs while the
# indexer writes old and new tables. Writes production, so the operator runs
# it, from the shell where `chw` / `chq` are defined (shell functions, hence
# `source`):
#
#   source <path>/fill_operations.zsh 100 101 102
#
# Items:
#   P        whole partition P (ledgers P*500000 .. P*500000+499999)
#   P:A      resume partition P at slice A (a multiple of 50,000 inside P)
#   A-B      explicit ledger range [A, B) — the head's partition, up to the
#            first dual-written ledger; B is exclusive
#
# Per slice [a, a+50000) (clipped to B in range mode), for each pair: the fill
# SQL through chw, then the gate through chq — distinct keys of the old table
# against the new one, per quarter slice so no query nears the memory cap.
# Stops at the first error or mismatch, naming the slice and the table.
# Re-running a slice is safe (the ReplacingMergeTrees collapse repeated rows).
# Stops before any partition when free disk is under 120 GiB.
() {
  local N=${1:A:h} item P A B a b from to out free g q lo hi pair new_t old_t old_k new_k sql old new
  shift
  local -a pairs=(
    'operations_appearances|transaction_operations|ledger_sequence, transaction_id, application_order|ledger_sequence, application_order, operation_index|fill_transaction_operations.sql'
    'lp_operation_amounts|pool_operation_amounts|pool_id, ledger_sequence, transaction_id, application_order, asset_id|pool_id, ledger_sequence, application_order, operation_index, asset_id|fill_pool_operation_amounts.sql'
  )
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
      for pair in $pairs; do
        old_t=${pair%%|*}; pair=${pair#*|}
        new_t=${pair%%|*}; pair=${pair#*|}
        old_k=${pair%%|*}; pair=${pair#*|}
        new_k=${pair%%|*}; sql=${pair#*|}
        # A successful INSERT prints nothing. Anything else — a server error,
        # or a transport error from curl on stderr — stops the loop.
        out=$(chw "$(sed -e "s/{A}/$a/g" -e "s/{B}/$b/g" "$N/$sql")" 2>&1)
        if (( $? )) || [[ -n $out ]]; then
          print -r -- "FAILED fill $new_t [$a, $b): $out"
          return 1
        fi
        old=0; new=0
        for q in 0 1 2 3; do
          lo=$((a + (b - a) * q / 4)); hi=$((a + (b - a) * (q + 1) / 4))
          # stdout only: a shell warning on stderr must not read as a count.
          g=$(chq "$(sed -e "s/{TBL}/$old_t/g" -e "s/{K}/$old_k/g" -e "s/{LO}/$lo/g" -e "s/{HI}/$hi/g" "$N/gate_operations.sql")" 2>/dev/null)
          [[ $g == <-> ]] || { print -r -- "FAILED gate $old_t [$lo, $hi): '$g'"; return 1; }
          old=$((old + g))
          g=$(chq "$(sed -e "s/{TBL}/$new_t/g" -e "s/{K}/$new_k/g" -e "s/{LO}/$lo/g" -e "s/{HI}/$hi/g" "$N/gate_operations.sql")" 2>/dev/null)
          [[ $g == <-> ]] || { print -r -- "FAILED gate $new_t [$lo, $hi): '$g'"; return 1; }
          new=$((new + g))
        done
        if (( old != new )); then
          print -r -- "FAILED gate $new_t [$a, $b): old keys $old, new keys $new"
          return 1
        fi
      done
    done
    print -r -- "ok $item $(date +%H:%M) free ${free} GiB"
  done
} "${(%):-%x}" "$@"
