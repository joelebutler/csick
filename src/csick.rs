use log::{debug, info, warn};
use serde::{Deserialize, Serialize};
use std::{io::Error, path::PathBuf};

pub mod cli;
pub mod generate;
pub mod mapping;
pub mod parsing;
pub mod setup;
pub mod types;
pub mod watch;

/// # look
/// Run analysis, but do not update.
/// ## Arguments
/// * `path` - Path to csick.json.
pub fn look(path: PathBuf, csick: bool, csickd: bool) -> Result<Vec<types::Function>, Error> {
    fn print_filtered(header: &str, filtered: Vec<&types::Function>) {
        println!("\n{:=^30}", header);
        if filtered.len() == 0 {
            println!("None found.\n");
            return;
        }
        println!("\n.");
        for (i, func) in filtered.iter().enumerate() {
            let last = i == filtered.len() - 1;
            let (branch, pipe) = if last {
                ("└──", "   ")
            } else {
                ("├──", "│  ")
            };
            println!(
                "{} {} {}({})",
                branch,
                func.cpp_return_type,
                func.cpp_name,
                generate::cpp_param_str(&func.cpp_params)
            );
            println!(
                "{}  └── {}({}) -> {}",
                pipe,
                func.rust_name,
                generate::rust_param_str(&func.rust_params),
                func.rust_return_type,
            );
        }
        println!();
    }

    println!("Beginning analysis...");

    let config = setup::Config::get(&path)?;
    info!("Successfully gathered config.");

    let parsed_functions = parsing::parse(&config.source_path, &config.additional_mappings);
    info!("Finished parsing functions.");

    if csick {
        let csick_only: Vec<_> = parsed_functions
            .iter()
            .filter(|f| f.annotation == CSICKAnnotation::CSICK)
            .collect();
        print_filtered(" CSICK functions ", csick_only);
    }

    if csickd {
        let csickd_only: Vec<_> = parsed_functions
            .iter()
            .filter(|f| f.annotation == CSICKAnnotation::CSICKD)
            .collect();
        print_filtered(" CSICKD functions ", csickd_only);
    }
    Ok(parsed_functions)
}

/// # go
/// Public function for running the CSick main process.
///
/// ## Arguments
/// * `p` - PathBuf of the folder or file to run the standard operation on.
pub fn go(path: PathBuf) -> std::io::Result<()> {
    println!("Beginning run...");

    let mut config = setup::Config::get(&path)?;
    info!("Successfully gathered config.");
    let mut parsed_functions = parsing::parse(&config.source_path, &config.additional_mappings);
    info!("Finished parsing functions.");

    for func in &mut parsed_functions {
        match func.annotation {
            CSICKAnnotation::CSICK => {
                // Create new
                debug!(
                    "START modifying {} in {}...",
                    func.cpp_name,
                    func.declaration_file.display()
                );
                let mut header_contents: String = std::fs::read_to_string(&func.declaration_file)?;
                generate::modify_cpp_declaration(&mut header_contents, func)?;
                generate::strip_existing_definition(func)?;
                setup::write_if_changed(&func.declaration_file, header_contents)?;
                debug!(
                    " DONE modifying {} in {}...",
                    func.cpp_name,
                    func.declaration_file.display()
                );

                let rust_file = func
                    .declaration_file
                    .with_file_name(parsing::rust_sanitize(
                        func.declaration_file
                            .file_stem()
                            .and_then(|s| s.to_str())
                            .unwrap_or_default(),
                    ))
                    .with_extension("rs");
                debug!(
                    "START modifying {} in {}...",
                    func.rust_name,
                    rust_file.display()
                );
                let mut rust_contents: String =
                    std::fs::read_to_string(&rust_file).unwrap_or_default();
                rust_contents.push_str(&generate::make_rust_function(func));
                rust_contents.push_str("\n");
                setup::write_if_changed(&rust_file, &rust_contents)?;
                debug!(
                    " DONE modifying {} in {}...",
                    func.rust_name,
                    rust_file.display()
                );

                config.sick_functions.push(func.clone());
            }
            CSICKAnnotation::CSICKD => {
                // Modify existing
                let old = config
                    .sick_functions
                    .iter()
                    .find(|f| f.unique_id == func.unique_id)
                    .cloned();
                let Some(old) = old else {
                    // Skip out if there isn't a sick_entry for it.
                    warn!("No sick_functions entry found for {}.", &func.unique_id);
                    continue;
                };

                let rust_file = func
                    .declaration_file
                    .with_file_name(parsing::rust_sanitize(
                        func.declaration_file
                            .file_stem()
                            .and_then(|s| s.to_str())
                            .unwrap_or_default(),
                    ))
                    .with_extension("rs");

                let cpp_changed = func.cpp_name != old.cpp_name
                    || func.cpp_params != old.cpp_params
                    || func.cpp_return_type != old.cpp_return_type;

                if cpp_changed {
                    // Update rust to match
                    info!("Updating {} in {}...", func.rust_name, rust_file.display());
                    let mut rust_contents = std::fs::read_to_string(&rust_file)?;
                    generate::modify_rust_function(&mut rust_contents, &old, func)?;
                    setup::write_if_changed(&rust_file, rust_contents)?;

                    if let Some(entry) = config
                        .sick_functions
                        .iter_mut()
                        .find(|f| f.unique_id == func.unique_id)
                    {
                        *entry = func.clone();
                    }
                }
            }
            CSICKAnnotation::NONE => {}
        }
    }

    // Write csick.h
    let csick_h: String = format!(
        "{}\n{}\n{}\n\n{}\n",
        setup::CSICK_H,
        generate::make_cpp_additional_includes(&config.additional_includes),
        generate::make_extern_block(&parsed_functions),
        generate::make_cpp_definition_block(&parsed_functions)
    );
    info!("Writing {}", &config.csick_h_path.display());
    setup::write_if_changed(&config.csick_h_path, csick_h)?;

    // Write csick.rs
    let csick_rs: String = format!(
        "{}\n{}\n",
        setup::CSICK_RS,
        generate::make_rust_bridge(&parsed_functions, &config.source_path)
    );
    setup::write_if_changed(&config.source_path.join("csick.rs"), csick_rs)?;

    // Write lib.rs
    let source = std::fs::canonicalize(&config.source_path).unwrap();
    let mut files: Vec<PathBuf> = parsed_functions
        .iter()
        .map(|f| f.declaration_file.clone())
        .collect();
    files.sort();
    files.dedup();

    let mut all_components: Vec<Vec<String>> = files
        .iter()
        .filter_map(|file| {
            let canonical = std::fs::canonicalize(file).ok()?;
            let sanitized = canonical.with_file_name(parsing::rust_sanitize(
                file.file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or_default(),
            ));
            let relative = sanitized.strip_prefix(&source).ok()?.with_extension("");
            Some(
                relative
                    .iter()
                    .map(|c| c.to_str().unwrap_or_default().to_string())
                    .collect(),
            )
        })
        .collect();
    all_components.sort();
    all_components.dedup();

    let lib_rs_content = format!(
        "// Auto-generated by csick. Do not edit manually.\n{}\n{}\n",
        setup::LIB_RS,
        generate::render_mods(&all_components, 0).join("\n")
    );
    let lib_rs_path = config.source_path.join("lib.rs");
    info!("Writing {}", lib_rs_path.display());
    setup::write_if_changed(&lib_rs_path, lib_rs_content)?;
    config.write(&path)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn go_creates_all_outputs_for_new_function() {
        in_temp_dir("csick_go_new", || {
            init_test_project("./src");
            setup::write_if_changed(
                "./src/math.h",
                "#include \"csick.h\"\nCSICK int add(int a, int b);",
            )
            .unwrap();

            go(PathBuf::from("./csick.json")).unwrap();

            let header = std::fs::read_to_string("./src/math.h").unwrap();
            assert!(
                header.contains("CSICKD("),
                "Header should have CSICKD annotation"
            );
            assert!(
                !header.contains("CSICK int add"),
                "Bare CSICK annotation should be replaced"
            );

            assert!(
                PathBuf::from("./src/math.rs").exists(),
                "Rust stub should be created"
            );
            let rust = std::fs::read_to_string("./src/math.rs").unwrap();
            assert!(rust.contains("pub fn add(a: i32, b: i32) -> i32"));
            assert!(rust.contains("/* csickd:"));
            assert!(rust.contains("todo!()"));

            let bridge = std::fs::read_to_string("./src/csick.rs").unwrap();
            assert!(bridge.contains("pub extern \"C\" fn"));
            assert!(bridge.contains("a: i32, b: i32"));

            let lib = std::fs::read_to_string("./src/lib.rs").unwrap();
            assert!(lib.contains("pub mod math"));

            let json = std::fs::read_to_string("./csick.json").unwrap();
            assert!(json.contains("\"add\""));
        });
    }

    fn init_test_project(source_rel: &str) {
        std::fs::create_dir_all(source_rel).unwrap();
        setup::init(
            PathBuf::from("./csick.json"),
            PathBuf::from(source_rel),
            None,
            None,
            false,
            false,
        )
        .unwrap();
    }

    #[test]
    fn look_fails_without_init() {
        in_temp_dir("csick_look_no_init", || {
            let err = look(PathBuf::from("./csick.json"), true, true).unwrap_err();
            assert_eq!(err.kind(), std::io::ErrorKind::NotFound);
            assert_eq!(
                err.to_string(),
                "CSICK not initialized. Run `csick init` first."
            );
        });
    }

    #[test]
    fn look_succeeds_with_empty_source() {
        in_temp_dir("csick_look_empty", || {
            init_test_project("./src");
            let result = look(PathBuf::from("./csick.json"), true, true).unwrap();
            assert!(result.is_empty());
        });
    }

    #[test]
    fn look_csick_flag_finds_csick_functions() {
        in_temp_dir("csick_look_csick_flag", || {
            init_test_project("./src");
            setup::write_if_changed(
                "./src/math.h",
                "#include \"csick.h\"\nCSICK int add(int a, int b);",
            )
            .unwrap();
            let result = look(PathBuf::from("./csick.json"), true, false).unwrap();
            assert_eq!(result.len(), 1);
            assert_eq!(result[0].annotation, CSICKAnnotation::CSICK);
            assert_eq!(result[0].cpp_name, "add");
        });
    }

    #[test]
    fn look_csickd_flag_finds_csickd_functions() {
        in_temp_dir("csick_look_csickd_flag", || {
            init_test_project("./src");
            setup::write_if_changed(
                "./src/math.h",
                "#include \"csick.h\"\nCSICKD(abc123) int add(int a, int b);",
            )
            .unwrap();
            let result = look(PathBuf::from("./csick.json"), false, true).unwrap();
            assert_eq!(result.len(), 1);
            assert_eq!(result[0].annotation, CSICKAnnotation::CSICKD);
            assert_eq!(result[0].cpp_name, "add");
            assert_eq!(result[0].unique_id, "abc123");
        });
    }

    #[test]
    fn look_both_flags_finds_mixed_functions() {
        in_temp_dir("csick_look_both_flags", || {
            init_test_project("./src");
            setup::write_if_changed(
                "./src/math.h",
                "#include \"csick.h\"\nCSICK int add(int a, int b);\nCSICKD(xyz789) float scale(float x);",
            )
            .unwrap();
            let result = look(PathBuf::from("./csick.json"), true, true).unwrap();
            assert_eq!(result.len(), 2);
            assert!(
                result
                    .iter()
                    .any(|f| f.annotation == CSICKAnnotation::CSICK)
            );
            assert!(
                result
                    .iter()
                    .any(|f| f.annotation == CSICKAnnotation::CSICKD)
            );
        });
    }
}

// Shared lock for all tests that use clang or mutate the process cwd.
// clang-sys uses thread-local storage for its loaded library handle and only
// allows one Clang instance globally, so every test that calls parse() or
// go() must hold this lock and call clang_sys::load() on its thread first.
#[cfg(test)]
fn test_lock() -> &'static std::sync::Mutex<()> {
    use std::sync::{Mutex, OnceLock};
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}
#[cfg(test)]
fn with_clang<F: FnOnce()>(f: F) {
    let _guard = test_lock().lock().unwrap_or_else(|e| e.into_inner());
    clang_sys::load().ok();
    f();
}
#[cfg(test)]
fn in_temp_dir<F: FnOnce()>(name: &str, f: F) {
    let _guard = test_lock().lock().unwrap_or_else(|e| e.into_inner());
    clang_sys::load().ok();
    let tmp = std::env::temp_dir().join(name);
    std::fs::remove_dir_all(&tmp).ok();
    std::fs::create_dir_all(&tmp).unwrap();
    let original = std::env::current_dir().unwrap();
    std::env::set_current_dir(&tmp).unwrap();
    f();
    std::env::set_current_dir(&original).unwrap();
    std::fs::remove_dir_all(&tmp).ok();
}

#[derive(Default, Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum CSICKAnnotation {
    CSICK = 1,
    CSICKD = 2,
    #[default]
    NONE = 0,
}
