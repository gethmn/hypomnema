# SQLite

The embedded SQL database that ships with everything.

## WAL mode

Write-ahead logging is the standard journaling mode for any concurrent
SQLite workload. Reads don't block writes; writes don't block reads.

## Storage shape

A single `.sqlite` file holds all tables, indexes, and — with the right
extension loaded — vector data alongside the metadata.
