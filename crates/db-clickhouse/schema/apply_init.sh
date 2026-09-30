#!/bin/sh
# Applies init.sql to every explorer database named in EXPLORER_DATABASES
# (task 0553): mainnet's `default` and testnet's `testnet` share one schema, so
# a table added to init.sql reaches both on the next `docker compose up`.
# Arguments are the clickhouse-client connection flags; the compose file of
# each environment passes its own.
set -eu
for db in $EXPLORER_DATABASES; do
  clickhouse-client "$@" --query "CREATE DATABASE IF NOT EXISTS $db"
  clickhouse-client "$@" --database "$db" --multiquery --queries-file=/init.sql
  echo "init.sql applied to $db"
done
