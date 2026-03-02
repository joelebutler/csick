use crate::csick::types::Function;
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs,
    io::{Error, ErrorKind},
    path::{Path, PathBuf},
};

pub fn write_if_changed(path: impl AsRef<Path>, contents: impl AsRef<[u8]>) -> std::io::Result<()> {
    let path = path.as_ref();
    let contents = contents.as_ref();
    if fs::read(path).is_ok_and(|existing| existing == contents) {
        return Ok(());
    }
    fs::write(path, contents)
}

pub const CSICK_H: &str = include_str!("./example_files/csick_ex.h");
pub const CSICK_RS: &str = include_str!("./example_files/csick_ex.rs");
pub const LIB_RS: &str = include_str!("./example_files/lib_ex.rs");
pub const CMAKE_BLOCK: &str = include_str!("./example_files/cmake_ex.txt");
pub fn crate_text(name: &str) -> String {
    format!(
        r#"[package]
name = "{}"
version = "0.0.1"
edition = "2024"

[lib]
crate-type = ["staticlib"]

[dependencies]
"#,
        name
    )
}
#[derive(Serialize, Deserialize, Debug)]
pub struct Config {
    pub source_path: PathBuf,
    pub crate_name: String,
    pub csick_h_path: PathBuf,
    pub additional_includes: Vec<String>,
    #[serde(default)]
    pub additional_mappings: HashMap<String, String>,
    pub sick_functions: Vec<Function>,
    #[serde(skip, default)]
    _lock: Option<fs::File>,
}

impl Config {
    pub fn write(&self, path: &PathBuf) -> std::io::Result<()> {
        // Serialize to a JSON string and write to file in one go
        let data = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        write_if_changed(path, data)
    }
    pub fn get(path: &PathBuf) -> std::io::Result<Self> {
        if !path.exists() {
            return Err(Error::new(
                ErrorKind::NotFound,
                "CSICK not initialized. Run `csick init` first.",
            ));
        }
        let file = fs::File::open(path)?;
        file.try_lock_exclusive().map_err(|_| {
            Error::new(
                ErrorKind::WouldBlock,
                "Config is locked by another csick process.",
            )
        })?;
        let data = fs::read_to_string(path)?;
        let mut config: Self =
            serde_json::from_str(&data).map_err(|e| Error::new(ErrorKind::Other, e))?;
        config._lock = Some(file);
        Ok(config)
    }
}

impl Drop for Config {
    fn drop(&mut self) {
        if let Some(f) = &self._lock {
            f.unlock().ok();
        }
    }
}

pub fn init(
    location: PathBuf,
    source_path: PathBuf,
    csick_h_path: Option<PathBuf>,
    cmake_path: Option<PathBuf>,
    cmake: bool,
    force: bool,
) -> Result<(), Error> {
    println!("Initializing...");
    let csick_h_path = csick_h_path.unwrap_or(source_path.join("csick.h"));
    let cmake_path = cmake_path.unwrap_or(PathBuf::from("./CMakeLists.txt"));
    let crate_name = PathBuf::from("./")
        .canonicalize()
        .ok()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
        .unwrap_or_else(|| "unknown_crate".to_string())
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect::<String>();
    // Check cmake before everything as this one is dual-purpose.
    if cmake {
        if let Some(mut contents) = std::fs::read_to_string(&cmake_path).ok() {
            // If CMake is true and isn't already generated, generate the CMakeLists insertion, regardless of force.
            if contents.find(CMAKE_BLOCK).is_none() {
                contents.insert_str(0, &CMAKE_BLOCK);
                write_if_changed(&cmake_path, contents)?;
            }
        } else {
            // Otherwise, just write the block to a new CMakeLists file.
            write_if_changed(&cmake_path, &CMAKE_BLOCK)?;
        }
    }

    let location = std::fs::canonicalize(&location).unwrap_or(location);
    let source_parent = source_path
        .parent()
        .ok_or_else(|| {
            Error::new(
                ErrorKind::NotFound,
                format!(
                    "Unable to access source parent. Source directory used: {}",
                    source_path.display()
                ),
            )
        })?
        .to_owned();
    let cargo_path = source_parent.join("Cargo.toml");

    if (location.exists() || cargo_path.exists()) && !force {
        // If either the location or cargo path exist and it's not forced, throw an error to inform the user.
        if location.exists() {
            return Err(Error::new(
                ErrorKind::AlreadyExists,
                format!(
                    "csick.json already exists at {}.  Run again with --force to overwrite existing initialization.",
                    location.display()
                ),
            ));
        } else {
            return Err(Error::new(
                ErrorKind::AlreadyExists,
                format!(
                    "Cargo.toml already exists at {}.  Run again with --force to initialize, overwriting the existing file.",
                    cargo_path.display()
                ),
            ));
        }
    }

    // Start generating the new config.

    if !source_path.is_dir() {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            format!(
                "Provided source path is not a directory: {}",
                source_path.display()
            ),
        ));
    }

    let config = Config {
        source_path,
        crate_name: crate_name.clone(),
        csick_h_path,
        additional_includes: Vec::new(),
        additional_mappings: HashMap::new(),
        sick_functions: Vec::new(),
        _lock: None,
    };

    write_if_changed(&config.csick_h_path, CSICK_H)?;
    write_if_changed(source_parent.join("Cargo.toml"), crate_text(&crate_name))?;
    config.write(&location)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn setup_early_get_fails() {
        crate::csick::in_temp_dir("csick_setup_no_init", || {
            let config_path = PathBuf::from("./csick.json");

            let too_early = Config::get(&config_path).unwrap_err();
            assert_eq!(too_early.kind(), std::io::ErrorKind::NotFound);
            assert_eq!(
                too_early.to_string(),
                "CSICK not initialized. Run `csick init` first."
            );
        });
    }

    #[test]
    fn setup_writes_and_loads() {
        crate::csick::in_temp_dir("csick_setup_config", || {
            let config_path = PathBuf::from("./csick.json");

            Config {
                source_path: PathBuf::from("./src"),
                crate_name: "test_crate".to_string(),
                csick_h_path: PathBuf::from("./src/csick.h"),
                additional_includes: vec!["<string>".to_string()],
                additional_mappings: HashMap::new(),
                sick_functions: vec![],
                _lock: None,
            }
            .write(&config_path)
            .unwrap();

            let loaded = Config::get(&config_path).unwrap();
            assert_eq!(loaded.source_path, PathBuf::from("./src"));
            assert_eq!(loaded.csick_h_path, PathBuf::from("./src/csick.h"));
            assert_eq!(loaded.additional_includes, vec!["<string>"]);
            assert!(loaded._lock.is_some());
        });
    }

    #[test]
    fn setup_init_creates_files() {
        crate::csick::in_temp_dir("csick_setup_init", || {
            std::fs::create_dir_all("./src").unwrap();
            let config_path = PathBuf::from("./csick.json");
            let source_path = PathBuf::from("./src");
            let csick_h_path = PathBuf::from("./src/csick.h");

            init(config_path.clone(), source_path, None, None, true, false).unwrap();

            // Make sure it writes (contents are checked elsewhere).
            assert!(config_path.exists());

            // csick.h checks
            assert!(csick_h_path.exists());
            let csick_h_contents = fs::read_to_string(csick_h_path).unwrap();
            assert!(csick_h_contents.contains(CSICK_H));

            // Cargo.toml checks
            let cargo_toml_path = PathBuf::from("Cargo.toml");
            assert!(cargo_toml_path.exists());
            let cargo_toml_contents = fs::read_to_string(cargo_toml_path).unwrap();
            assert!(!cargo_toml_contents.is_empty());

            let cmake_contents = std::fs::read_to_string("./CMakeLists.txt").unwrap();
            assert!(cmake_contents.contains(CMAKE_BLOCK));
        });
    }

    #[test]
    fn setup_init_must_force() {
        crate::csick::in_temp_dir("csick_setup_force", || {
            std::fs::create_dir_all("./src").unwrap();
            let config_path = PathBuf::from("./csick.json");
            let source_path = PathBuf::from("./src");
            let cmake_path = PathBuf::from("./CMakeLists.txt");

            init(
                config_path.clone(),
                source_path.clone(),
                None,
                Some(cmake_path.clone()),
                true,
                false,
            )
            .unwrap();
            let no_force = init(
                config_path.clone(),
                source_path.clone(),
                None,
                Some(cmake_path.clone()),
                true,
                false,
            )
            .unwrap_err();

            let abs_path = std::fs::canonicalize(&config_path).unwrap_or(config_path.clone());
            assert_eq!(no_force.kind(), std::io::ErrorKind::AlreadyExists);
            assert_eq!(
                no_force.to_string(),
                format!(
                    "csick.json already exists at {}.  Run again with --force to overwrite existing initialization.",
                    abs_path.display()
                ),
            );

            let forced = init(
                config_path.clone(),
                source_path,
                None,
                Some(cmake_path.clone()),
                true,
                true,
            );
            assert!(forced.is_ok());
            forced.unwrap();
        });
    }
}
