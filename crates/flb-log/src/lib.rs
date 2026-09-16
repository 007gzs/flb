use chrono::{DateTime, Local, NaiveDate};
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError, SyncSender};
use std::thread;
use std::time::Duration;
use tracing_subscriber::fmt::MakeWriter;

const QUEUE_CAP: usize = 16_384;
const MAX_SIZE: u64 = 100 * 1024 * 1024;

#[derive(Clone)]
pub struct FileLogger {
    tx: SyncSender<Vec<u8>>,
}

impl FileLogger {
    pub fn open(path: impl AsRef<Path>) -> io::Result<Self> {
        let path = path.as_ref();
        let dir = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf();
        fs::create_dir_all(&dir)?;
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("log")
            .to_string();
        let (tx, rx) = mpsc::sync_channel::<Vec<u8>>(QUEUE_CAP);
        thread::Builder::new()
            .name(format!("flb-log-{stem}"))
            .spawn(move || writer_loop(dir, stem, rx))?;
        Ok(Self { tx })
    }

    pub fn line(&self, line: impl AsRef<str>) {
        let mut buf = line.as_ref().as_bytes().to_vec();
        if !buf.ends_with(b"\n") {
            buf.push(b'\n');
        }
        self.write_bytes(buf);
    }

    fn write_bytes(&self, buf: Vec<u8>) {
        let _ = self.tx.try_send(buf);
    }
}

impl<'a> MakeWriter<'a> for FileLogger {
    type Writer = LogWriter;

    fn make_writer(&'a self) -> Self::Writer {
        LogWriter {
            inner: self.clone(),
        }
    }
}

pub struct LogWriter {
    inner: FileLogger,
}

impl Write for LogWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if !buf.is_empty() {
            self.inner.write_bytes(buf.to_vec());
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

struct RotatingFile {
    dir: PathBuf,
    stem: String,
    date: NaiveDate,
    written: u64,
    out: BufWriter<File>,
}

impl RotatingFile {
    fn open(dir: PathBuf, stem: String) -> io::Result<Self> {
        let (out, written, date) = open_current(&dir, &stem)?;
        let mut file = Self {
            dir,
            stem,
            date,
            written,
            out,
        };
        file.maybe_rotate(0)?;
        Ok(file)
    }

    fn write_all(&mut self, buf: &[u8]) -> io::Result<()> {
        self.maybe_rotate(buf.len() as u64)?;
        self.out.write_all(buf)?;
        self.written += buf.len() as u64;
        Ok(())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.out.flush()
    }

    fn maybe_rotate(&mut self, extra: u64) -> io::Result<()> {
        let today = Local::now().date_naive();
        let new_day = today != self.date;
        let too_big = self.written > 0 && self.written.saturating_add(extra) >= MAX_SIZE;
        if (new_day || too_big) && self.written > 0 {
            self.rotate()?;
        } else if new_day {
            self.date = today;
        }
        Ok(())
    }

    fn rotate(&mut self) -> io::Result<()> {
        self.out.flush()?;
        let current = current_path(&self.dir, &self.stem);
        if current.exists() {
            let dest = archive_path(&self.dir, &self.stem, self.date);
            fs::rename(&current, dest)?;
        }
        let (out, written, date) = open_current(&self.dir, &self.stem)?;
        self.out = out;
        self.written = written;
        self.date = date;
        Ok(())
    }
}

fn current_path(dir: &Path, stem: &str) -> PathBuf {
    dir.join(format!("{stem}.log"))
}

fn archive_path(dir: &Path, stem: &str, date: NaiveDate) -> PathBuf {
    let date = date.format("%Y-%m-%d");
    let base = dir.join(format!("{stem}.{date}.log"));
    if !base.exists() {
        return base;
    }
    for seq in 1..u32::MAX {
        let candidate = dir.join(format!("{stem}.{date}.{seq}.log"));
        if !candidate.exists() {
            return candidate;
        }
    }
    dir.join(format!("{stem}.{date}.overflow.log"))
}

fn open_current(dir: &Path, stem: &str) -> io::Result<(BufWriter<File>, u64, NaiveDate)> {
    let path = current_path(dir, stem);
    let file = OpenOptions::new().create(true).append(true).open(&path)?;
    let meta = file.metadata()?;
    let date = meta
        .modified()
        .ok()
        .map(|time| DateTime::<Local>::from(time).date_naive())
        .unwrap_or_else(|| Local::now().date_naive());
    Ok((BufWriter::with_capacity(64 * 1024, file), meta.len(), date))
}

fn writer_loop(dir: PathBuf, stem: String, rx: mpsc::Receiver<Vec<u8>>) {
    let Ok(mut file) = RotatingFile::open(dir, stem) else {
        return;
    };
    loop {
        match rx.recv_timeout(Duration::from_millis(200)) {
            Ok(chunk) => {
                let _ = file.write_all(&chunk);
                while let Ok(more) = rx.try_recv() {
                    let _ = file.write_all(&more);
                }
            }
            Err(RecvTimeoutError::Timeout) => {
                let _ = file.flush();
                let _ = file.maybe_rotate(0);
            }
            Err(RecvTimeoutError::Disconnected) => {
                let _ = file.flush();
                break;
            }
        }
    }
}

pub fn init_tracing(error_log: FileLogger) {
    use tracing_subscriber::EnvFilter;
    use tracing_subscriber::filter::LevelFilter;
    use tracing_subscriber::fmt;
    use tracing_subscriber::prelude::*;

    let env = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::registry()
        .with(env)
        .with(fmt::layer().with_writer(std::io::stderr))
        .with(
            fmt::layer()
                .with_ansi(false)
                .with_writer(error_log)
                .with_filter(LevelFilter::WARN),
        )
        .init();
}

#[cfg(test)]
mod tests {
    use super::archive_path;
    use chrono::NaiveDate;
    use std::fs;

    #[test]
    fn archive_path_increments_when_taken() {
        let dir = tempfile::tempdir().unwrap();
        let date = NaiveDate::from_ymd_opt(2026, 9, 16).unwrap();
        let first = archive_path(dir.path(), "access", date);
        assert_eq!(first.file_name().unwrap(), "access.2026-09-16.log");
        fs::write(&first, b"x").unwrap();
        let second = archive_path(dir.path(), "access", date);
        assert_eq!(second.file_name().unwrap(), "access.2026-09-16.1.log");
    }
}
