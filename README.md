# LogSpindle

LogSpindle is a Rust command line utility for inspecting newline-delimited JSON application logs. It filters
records and summarizes severity and error-code frequencies using a streaming reader. It needs no server, database,
or account.

This is a local diagnostic tool, not a log collector, monitoring platform, or security scanner.

## Input format

Each line must contain one UTF-8 JSON object with these required fields:

| Field | Type | Meaning |
| --- | --- | --- |
| `timestamp` | String | RFC 3339 timestamp with timezone offset |
| `level` | String | TRACE, DEBUG, INFO, WARN, WARNING, ERROR, or FATAL |
| `message` | String | Message searched by `--contains` |
| `code` | String or null | Optional error identifier used in summaries |

Additional JSON fields are permitted. Error and fatal records without a nonempty code are grouped under
`(uncategorized)`.

Example:

```json
{"timestamp":"2026-01-01T00:00:02Z","level":"ERROR","message":"connection timeout","code":"network_timeout"}
```

## Usage

Rust 1.85 or newer is required.

```sh
cargo run -- summary tests/fixtures/sample.jsonl
cargo run -- summary tests/fixtures/sample.jsonl --min-level error --top 5 --json
cargo run -- filter tests/fixtures/sample.jsonl --contains timeout
cargo run -- filter tests/fixtures/sample.jsonl --since 2026-01-01T00:00:02Z
```

Read from standard input by omitting the filename or using `-`. Timestamp bounds are inclusive and compare absolute
instants, including across different time offsets. `--contains` is case-sensitive. `--min-level` includes all
higher severities.

By default, invalid records are counted and skipped. A single warning reports the total count to standard error.
Use `--strict` to fail at the first invalid record, including invalid timestamps, missing fields, unsupported
levels, or invalid JSON.

`summary --json` emits a JSON object with `total_lines`, `valid_entries`, `malformed_entries`, `matched_entries`,
`levels`, and `top_error_codes`. The level counts include matching records only. Error codes are sorted by
descending frequency and then alphabetically for ties.

`filter` emits original matching JSONL records on standard output. This output may contain sensitive log data. It
writes records as it reads them. In strict mode, records written before a later error cannot be retracted.

## Limitations

- Only the documented JSONL schema is supported. Plain text, multiline logs, and compressed files are not decoded.
- The program reads one line at a time but may retain distinct error codes for summary counts. Memory use grows
  with the longest line and the number of different codes.
- Files must be valid UTF-8. Read failures stop processing.
- It does not scrub secrets or personally identifiable content from input records or error codes. Review logs
  before sharing exports.
- It does not follow files, watch directories, rotate logs, or make network requests.
- The sample file deliberately includes an invalid final line to exercise error handling.

## Validation

Run the independent text gate first:

```sh
python3 scripts/check_text.py
```

Then follow [the local validation contract](docs/VALIDATION.md) for Rust formatting, Clippy, tests, release build,
and verification evidence. The scripts in `scripts/` are for local use and do not require hosted automation.

Source is licensed under [MIT](LICENSE).
