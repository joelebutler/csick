use crate::csick::{go, setup::Config};
use log::error;
use notify_debouncer_mini::{DebounceEventResult, new_debouncer, notify::*};
use std::{collections::HashSet, path::PathBuf, sync::mpsc::channel, time::Duration};

/// # watch
/// Run go on relevant file saves.
/// ## Arguments
/// * `path` - Path to csick.json.
pub fn watch(config_path: PathBuf) -> std::io::Result<()> {
    let (source_path, ignore_paths) = {
        let config = Config::get(&config_path)?;
        let ignore: HashSet<PathBuf> = [
            config.csick_h_path.clone(),
            config.source_path.join("csick.rs"),
            config.source_path.join("lib.rs"),
            config_path.clone(),
        ]
        .into_iter()
        .map(|p| p.canonicalize().unwrap_or(p))
        .collect();
        (config.source_path.clone(), ignore)
    };

    go(config_path.clone())?;

    let (tx, rx) = channel::<DebounceEventResult>();

    let mut debouncer = new_debouncer(
        Duration::from_millis(300),
        move |res: DebounceEventResult| {
            tx.send(res).ok();
        },
    )
    .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;

    debouncer
        .watcher()
        .watch(&source_path, RecursiveMode::Recursive)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;

    println!(
        "Watching {} for changes. Press Ctrl+C to stop.",
        source_path.display()
    );

    for result in rx {
        match result {
            Ok(events) => {
                let relevant = events.iter().any(|e| {
                    let canonical = e.path.canonicalize().unwrap_or_else(|_| e.path.clone());
                    !ignore_paths.contains(&canonical)
                        && e.path
                            .extension()
                            .and_then(|ext| ext.to_str())
                            .map(|ext| ext == "h" || ext == "cpp" || ext == "rs")
                            .unwrap_or(false)
                });
                if relevant {
                    if let Err(e) = go(config_path.clone()) {
                        error!("csick error: {e}");
                    }
                }
            }
            Err(e) => error!("Watch error: {e}"),
        }
    }

    Ok(())
}
