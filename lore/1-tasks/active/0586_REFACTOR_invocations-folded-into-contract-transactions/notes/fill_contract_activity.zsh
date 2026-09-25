# Task 0586: fill contract_activity from contract_transactions and
# soroban_invocations_appearances, slice by slice, gating each slice. Runs
# while the indexer writes old and new tables. Writes production, so the
# operator runs it, from the shell where `chw` / `chq` are defined (shell
# functions, hence `source`):
#
#   source <path>/fill_contract_activity.zsh 100 101 102
#
# Items:
#   P        whole partition P (ledgers P*500000 .. P*500000+499999)
#   P:A      resume partition P at slice A (a multiple of 10,000 inside P)
#   A-B      explicit ledger range [A, B) — the head's partition, up to the
#            first dual-written ledger; B is exclusive
#
# Per slice [a, a+10000) (clipped to B in range mode): the fill SQL through
# chw, then the gate through chq (gate_contract_activity.sql). Stops at the
# first error or mismatch, naming the slice. 10,000 ledgers, not 50,000: a
# 50,000-ledger fill SELECT exceeded the 3.73 GiB memory cap.
# Re-running a slice is safe (the ReplacingMergeTree collapses repeated rows).
# Stops before any partition when free disk is under 120 GiB.
() {
  local N=${1:A:h} item P A B a b from to out free g
  local -a n
  shift
  for item in "$@"; do
    if [[ $item == <->-<-> ]]; then
      from=${item%%-*}; to=${item#*-}
    elif [[ $item == <->:<-> ]]; then
      P=${item%%:*}; A=${item#*:}
      if (( A % 10000 || A / 500000 != P )); then
        print -r -- "STOP: resume point $A is not a 10,000-ledger slice of partition $P"
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
    for a in $(seq -f '%.0f' $from 10000 $((to - 1))); do
      b=$((a + 10000)); (( b > to )) && b=$to
      # A successful INSERT prints nothing. Anything else — a server error,
      # or a transport error from curl on stderr — stops the loop.
      out=$(chw "$(sed -e "s/{A}/$a/g" -e "s/{B}/$b/g" "$N/fill_contract_activity.sql")" 2>&1)
      if (( $? )) || [[ -n $out ]]; then
        print -r -- "FAILED fill [$a, $b): $out"
        return 1
      fi
      # stdout only: a shell warning on stderr must not read as a count.
      g=$(chq "$(sed -e "s/{LO}/$a/g" -e "s/{HI}/$b/g" "$N/gate_contract_activity.sql")" 2>/dev/null)
      n=(${=g})
      if (( ${#n} != 4 )) || [[ ${n[1]} != <-> ]]; then
        print -r -- "FAILED gate [$a, $b): '$g'"
        return 1
      fi
      if (( n[1] != n[2] || n[3] != n[4] )); then
        print -r -- "FAILED gate [$a, $b): presence ${n[1]} activity ${n[2]} invoked ${n[3]} callers ${n[4]}"
        return 1
      fi
    done
    print -r -- "ok $item $(date +%H:%M) free ${free} GiB"
  done
} "${(%):-%x}" "$@"
