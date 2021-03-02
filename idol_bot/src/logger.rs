use anyhow::Result;
use chrono::prelude::*;
use libsystemd::logging::{journal_send, Priority};
use log::*;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};

pub fn init() -> Result<()> {
    let journald_works = AtomicBool::new(true);
    let syslog = fern::Output::call(move |record| {
        if journald_works.load(Ordering::SeqCst) {
            let priority = match record.level() {
                Level::Error => Priority::Error,
                Level::Warn => Priority::Warning,
                Level::Info => Priority::Notice,
                Level::Debug => Priority::Info,
                Level::Trace => Priority::Debug,
            };
            let mut fields = HashMap::new();
            fields.insert("SYSLOG_IDENTIFIER", "idol_bot".to_string());
            if let Some(file) = record.file().or_else(|| record.module_path()) {
                fields.insert("CODE_FILE", file.to_string());
            }
            if let Some(line) = record.line() {
                fields.insert("CODE_LINE", line.to_string());
            }
            if let Err(err) = journal_send(priority, &record.args().to_string(), fields.into_iter())
            {
                journald_works.store(false, Ordering::SeqCst);
                warn!("journald error: {}", err);
            }
        }
    });
    fern::Dispatch::new()
        .level(log::LevelFilter::Warn)
        .level_for("idol_bot", log::LevelFilter::Trace)
        .level_for("idol_predictor", log::LevelFilter::Trace)
        .level_for("idol_api", log::LevelFilter::Trace)
        .level_for("tide", log::LevelFilter::Info)
        .chain(
            fern::Dispatch::new()
                .format(move |out, message, record| {
                    out.finish(format_args!(
                        "[{} {} {}] {}",
                        Utc::now().format("%Y-%m-%dT%H:%M:%SZ"),
                        record.level(),
                        record.target(),
                        message
                    ))
                })
                .chain(std::io::stdout()),
        )
        .chain(
            fern::Dispatch::new()
                .format(move |out, message, record| {
                    out.finish(format_args!(
                        "[{}] {}",
                        record.module_path().unwrap_or("<unknown>"),
                        message
                    ))
                })
                .chain(syslog),
        )
        .apply()?;
    Ok(())
}
