use std::collections::BTreeMap;
use std::error::Error;
use std::fs::File;
use std::io::{self, BufRead, BufReader, BufWriter, Write};

use chrono::{DateTime, FixedOffset};
use clap::{Args, Parser, Subcommand, ValueEnum};
use serde::{Deserialize, Serialize};

type AppResult<T> = Result<T, Box<dyn Error>>;

#[derive(Parser)]
#[command(name = "logspindle", version, about = "Inspect newline-delimited JSON application logs")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Count matching records and report error codes.
    Summary(SummaryOptions),
    /// Emit matching records as JSONL.
    Filter(Selection),
}

#[derive(Args)]
struct SummaryOptions {
    #[command(flatten)]
    selection: Selection,

    /// Maximum number of error codes to display.
    #[arg(long, default_value_t = 10)]
    top: usize,

    /// Emit machine-readable JSON instead of a text report.
    #[arg(long)]
    json: bool,
}

#[derive(Args)]
struct Selection {
    /// Input JSONL file, or - for standard input.
    #[arg(value_name = "FILE", default_value = "-")]
    input: String,

    /// Keep records at or above this severity.
    #[arg(long, value_enum)]
    min_level: Option<Level>,

    /// Inclusive RFC 3339 timestamp lower bound.
    #[arg(long, value_parser = parse_time)]
    since: Option<DateTime<FixedOffset>>,

    /// Inclusive RFC 3339 timestamp upper bound.
    #[arg(long, value_parser = parse_time)]
    until: Option<DateTime<FixedOffset>>,

    /// Case-sensitive substring to find in the message field.
    #[arg(long)]
    contains: Option<String>,

    /// Stop on the first malformed record (otherwise count and skip it).
    #[arg(long)]
    strict: bool,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, ValueEnum)]
enum Level {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
    Fatal,
}

impl Level {
    fn parse(value: &str) -> Option<Self> {
        match value.to_ascii_uppercase().as_str() {
            "TRACE" => Some(Self::Trace),
            "DEBUG" => Some(Self::Debug),
            "INFO" => Some(Self::Info),
            "WARN" | "WARNING" => Some(Self::Warn),
            "ERROR" => Some(Self::Error),
            "FATAL" => Some(Self::Fatal),
            _ => None,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Trace => "TRACE",
            Self::Debug => "DEBUG",
            Self::Info => "INFO",
            Self::Warn => "WARN",
            Self::Error => "ERROR",
            Self::Fatal => "FATAL",
        }
    }
}

#[derive(Deserialize)]
struct RawEntry {
    timestamp: String,
    level: String,
    message: String,
    #[serde(default)]
    code: Option<String>,
}

struct Entry {
    timestamp: DateTime<FixedOffset>,
    level: Level,
    message: String,
    code: Option<String>,
}

fn parse_time(raw: &str) -> Result<DateTime<FixedOffset>, String> {
    DateTime::parse_from_rfc3339(raw)
        .map_err(|_| format!("invalid RFC 3339 timestamp: {raw}"))
}

fn parse_entry(line: &str) -> Result<Entry, String> {
    let raw: RawEntry =
        serde_json::from_str(line).map_err(|error| format!("invalid JSON record: {error}"))?;
    let timestamp = DateTime::parse_from_rfc3339(&raw.timestamp)
        .map_err(|_| "timestamp must be RFC 3339 with an offset".to_owned())?;
    let level = Level::parse(&raw.level)
        .ok_or_else(|| "level must be TRACE, DEBUG, INFO, WARN, ERROR, or FATAL".to_owned())?;
    Ok(Entry {
        timestamp,
        level,
        message: raw.message,
        code: raw.code,
    })
}

fn matches_selection(entry: &Entry, selection: &Selection) -> bool {
    selection
        .min_level
        .is_none_or(|minimum| entry.level >= minimum)
        && selection
            .since
            .as_ref()
            .is_none_or(|since| &entry.timestamp >= since)
        && selection
            .until
            .as_ref()
            .is_none_or(|until| &entry.timestamp <= until)
        && selection
            .contains
            .as_ref()
            .is_none_or(|needle| entry.message.contains(needle))
}

#[derive(Default, Serialize)]
struct ScanCounts {
    total_lines: u64,
    valid_entries: u64,
    malformed_entries: u64,
    matched_entries: u64,
}

fn scan<F>(
    reader: &mut dyn BufRead,
    selection: &Selection,
    mut on_match: F,
) -> AppResult<ScanCounts>
where
    F: FnMut(&Entry, &str) -> io::Result<()>,
{
    if let (Some(since), Some(until)) = (&selection.since, &selection.until) {
        if since > until {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "--since must not be later than --until",
            )
            .into());
        }
    }

    let mut counts = ScanCounts::default();
    let mut line = String::new();
    loop {
        line.clear();
        if reader.read_line(&mut line)? == 0 {
            break;
        }
        counts.total_lines += 1;
        let entry = match parse_entry(&line) {
            Ok(entry) => entry,
            Err(reason) => {
                counts.malformed_entries += 1;
                if selection.strict {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("line {}: {reason}", counts.total_lines),
                    )
                    .into());
                }
                continue;
            }
        };
        counts.valid_entries += 1;
        if matches_selection(&entry, selection) {
            counts.matched_entries += 1;
            on_match(&entry, &line)?;
        }
    }
    Ok(counts)
}

#[derive(Serialize)]
struct ErrorCodeCount {
    code: String,
    count: u64,
}

#[derive(Serialize)]
struct Summary {
    total_lines: u64,
    valid_entries: u64,
    malformed_entries: u64,
    matched_entries: u64,
    levels: BTreeMap<String, u64>,
    top_error_codes: Vec<ErrorCodeCount>,
}

fn summarize(reader: &mut dyn BufRead, selection: &Selection, top: usize) -> AppResult<Summary> {
    let mut levels = BTreeMap::<String, u64>::new();
    let mut errors = BTreeMap::<String, u64>::new();

    let counts = scan(reader, selection, |entry, _| {
        *levels.entry(entry.level.as_str().to_owned()).or_default() += 1;
        if entry.level >= Level::Error {
            let code = entry
                .code
                .as_deref()
                .filter(|code| !code.trim().is_empty())
                .unwrap_or("(uncategorized)");
            *errors.entry(code.to_owned()).or_default() += 1;
        }
        Ok(())
    })?;

    let mut top_error_codes: Vec<ErrorCodeCount> = errors
        .into_iter()
        .map(|(code, count)| ErrorCodeCount { code, count })
        .collect();
    top_error_codes.sort_by(|left, right| {
        right
            .count
            .cmp(&left.count)
            .then_with(|| left.code.cmp(&right.code))
    });
    top_error_codes.truncate(top);

    Ok(Summary {
        total_lines: counts.total_lines,
        valid_entries: counts.valid_entries,
        malformed_entries: counts.malformed_entries,
        matched_entries: counts.matched_entries,
        levels,
        top_error_codes,
    })
}

fn render_summary<W: Write>(output: &mut W, summary: &Summary, json: bool) -> AppResult<()> {
    if json {
        serde_json::to_writer_pretty(&mut *output, summary)?;
        writeln!(output)?;
        return Ok(());
    }

    writeln!(output, "LogSpindle summary")?;
    writeln!(output, "Lines read:       {}", summary.total_lines)?;
    writeln!(output, "Valid entries:    {}", summary.valid_entries)?;
    writeln!(output, "Malformed:        {}", summary.malformed_entries)?;
    writeln!(output, "Matched entries:  {}", summary.matched_entries)?;
    writeln!(output, "Matching severity counts:")?;
    for level in [
        Level::Trace,
        Level::Debug,
        Level::Info,
        Level::Warn,
        Level::Error,
        Level::Fatal,
    ] {
        let name = level.as_str();
        writeln!(output, "  {name:<6} {}", summary.levels.get(name).unwrap_or(&0))?;
    }
    writeln!(output, "Top error codes:")?;
    for error in &summary.top_error_codes {
        // JSON quoting prevents log-controlled terminal escape sequences in text output.
        writeln!(
            output,
            "  {:>7}  {}",
            error.count,
            serde_json::to_string(&error.code)?
        )?;
    }
    Ok(())
}

fn open_reader(path: &str) -> io::Result<Box<dyn BufRead>> {
    if path == "-" {
        Ok(Box::new(BufReader::new(io::stdin())))
    } else {
        Ok(Box::new(BufReader::new(File::open(path)?)))
    }
}

fn report_skipped(counts: u64) {
    if counts > 0 {
        eprintln!("logspindle: skipped {counts} malformed record(s); use --strict to fail");
    }
}

fn run() -> AppResult<()> {
    let cli = Cli::parse();
    let stdout = io::stdout();
    let mut output = BufWriter::new(stdout.lock());

    match cli.command {
        Command::Summary(options) => {
            let mut reader = open_reader(&options.selection.input)?;
            let summary = summarize(&mut *reader, &options.selection, options.top)?;
            render_summary(&mut output, &summary, options.json)?;
            output.flush()?;
            report_skipped(summary.malformed_entries);
        }
        Command::Filter(selection) => {
            let mut reader = open_reader(&selection.input)?;
            let counts = scan(&mut *reader, &selection, |_, line| {
                output.write_all(line.as_bytes())?;
                if !line.ends_with('\n') {
                    output.write_all(b"\n")?;
                }
                Ok(())
            })?;
            output.flush()?;
            report_skipped(counts.malformed_entries);
        }
    }
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        // A closed downstream pipe is a normal result of piping through "head".
        if error
            .downcast_ref::<io::Error>()
            .is_some_and(|error| error.kind() == io::ErrorKind::BrokenPipe)
        {
            return;
        }
        eprintln!("logspindle: {error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn selection() -> Selection {
        Selection {
            input: "-".into(),
            min_level: None,
            since: None,
            until: None,
            contains: None,
            strict: false,
        }
    }

    #[test]
    fn offset_timestamps_compare_by_instant() {
        let mut options = selection();
        options.since = Some(parse_time("2026-01-01T01:00:00+01:00").unwrap());
        let record =
            parse_entry(r#"{"timestamp":"2026-01-01T00:00:00Z","level":"INFO","message":"ok"}"#)
                .unwrap();
        assert!(matches_selection(&record, &options));
    }

    #[test]
    fn warning_alias_is_accepted() {
        assert_eq!(Level::parse("warning"), Some(Level::Warn));
        assert!(Level::parse("other").is_none());
    }

    #[test]
    fn summary_ties_sort_by_code() {
        let input = concat!(
            "{\"timestamp\":\"2026-01-01T00:00:00Z\",\"level\":\"ERROR\",\"message\":\"a\",\"code\":\"Z\"}\n",
            "{\"timestamp\":\"2026-01-01T00:00:00Z\",\"level\":\"ERROR\",\"message\":\"b\",\"code\":\"A\"}\n",
        );
        let mut reader = BufReader::new(input.as_bytes());
        let result = summarize(&mut reader, &selection(), 10).unwrap();
        assert_eq!(result.top_error_codes[0].code, "A");
        assert_eq!(result.top_error_codes[1].code, "Z");
    }

    #[test]
    fn strict_rejects_malformed_without_printing_payload() {
        let mut options = selection();
        options.strict = true;
        let mut reader = BufReader::new(b"not json\n".as_slice());
        let error = summarize(&mut reader, &options, 10).err().unwrap();
        let message = error.to_string();
        assert!(message.contains("line 1"));
        assert!(!message.contains("not json"));
    }

    #[test]
    fn invalid_time_window_is_rejected() {
        let mut options = selection();
        options.since = Some(parse_time("2026-01-02T00:00:00Z").unwrap());
        options.until = Some(parse_time("2026-01-01T00:00:00Z").unwrap());
        let mut reader = BufReader::new(b"".as_slice());
        assert!(summarize(&mut reader, &options, 10).is_err());
    }
}
