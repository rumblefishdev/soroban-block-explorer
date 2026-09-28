# Phase 3 of task 0541: fill whole partitions of both tables, in the order given.
# Writes production, so the operator runs it, from the shell where `chw` is
# defined (it is a shell function, hence `source`, not `zsh`):
#
#   source <path>/fill_partitions.zsh 100 101 102
#
# Per partition: stop if free disk is under 120 GiB; rekey `soroban_events` into
# the staging table (fill_insert.sql), then `contract_transactions`
# (fill_contract_transactions.sql, which reads the rekeyed rows); 5k-ledger
# slices; stop at the first error, naming the file and the slice. A re-run of a
# slice is safe: the engine collapses the duplicate rows.
#
# Whole partitions below the head's only. The head's partition and the tail are
# filled with an explicit end (plan 4.2).
#
# To resume an interrupted partition, give `P:A` — the rekey restarts at slice
# `A` (a multiple of 5,000 inside `P`); `contract_transactions` still runs over
# the whole partition, since it only starts after the rekey.
() {
  local N=${1:A:h} P A a F out free from
  shift
  for P in "$@"; do
    A=
    if [[ $P == *:* ]]; then A=${P#*:}; P=${P%%:*}; fi
    if [[ -n $A ]] && { [[ $A != <-> ]] || (( A % 5000 || A / 500000 != P )); }; then
      print -r -- "STOP: resume point $A is not a 5,000-ledger slice of partition $P"
      return 1
    fi
    free=$(chq "SELECT intDiv(free_space, 1073741824) FROM system.disks WHERE name = 'default'")
    if [[ $free != <-> ]] || (( free < 120 )); then
      print -r -- "STOP before $P: free disk reads '$free' GiB"
      return 1
    fi
    for F in fill_insert.sql fill_contract_transactions.sql; do
      from=$((P * 500000))
      [[ $F == fill_insert.sql && -n $A ]] && from=$A
      for a in $(seq -f '%.0f' $from 5000 $((P * 500000 + 495000))); do
        # A successful INSERT prints nothing. Anything else — a server error, or
        # a transport error from curl on stderr (a timed-out slice once passed
        # as "ok") — stops the loop; re-run from this slice with `P:A`.
        out=$(chw "$(sed -e "s/{A}/$a/g" -e "s/{B}/$((a + 5000))/g" "$N/$F")" 2>&1)
        if (( $? )) || [[ -n $out ]]; then
          print -r -- "FAILED $P $F at $a: $out"
          return 1
        fi
      done
      print -r -- "ok $P $F $(date +%H:%M) free ${free} GiB"
    done
  done
} "${(%):-%x}" "$@"
