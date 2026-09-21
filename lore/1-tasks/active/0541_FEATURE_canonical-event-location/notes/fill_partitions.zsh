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
() {
  local N=${1:A:h} P a F out free
  shift
  for P in "$@"; do
    free=$(chq "SELECT intDiv(free_space, 1073741824) FROM system.disks WHERE name = 'default'")
    if [[ $free != <-> ]] || (( free < 120 )); then
      print -r -- "STOP before $P: free disk reads '$free' GiB"
      return 1
    fi
    for F in fill_insert.sql fill_contract_transactions.sql; do
      for a in $(seq -f '%.0f' $((P * 500000)) 5000 $((P * 500000 + 495000))); do
        out=$(chw "$(sed -e "s/{A}/$a/g" -e "s/{B}/$((a + 5000))/g" "$N/$F")")
        if print -r -- "$out" | grep -q "DB::Exception"; then
          print -r -- "FAILED $P $F at $a: $out"
          return 1
        fi
      done
      print -r -- "ok $P $F $(date +%H:%M) free ${free} GiB"
    done
  done
} "${(%):-%x}" "$@"
