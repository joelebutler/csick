use crate::csick;
use log::{debug, error, info, warn};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::LazyLock;

/// # parse
/// Performs all parsing operations on a file or directory.
///
/// ## Arguments
/// * `source` - file or directory to use as source to search.
///
/// ## Return
/// Vec<csick::types::Function> - Parsed functions from the read files.
pub fn parse(source: &PathBuf, additional_mappings: &HashMap<String, String>) -> Vec<csick::types::Function> {
    let mut funcs = parse_csick(Vec::new(), source, additional_mappings);
    funcs = parse_bodies(funcs, source);
    funcs
}

fn parse_common<
    F: FnMut(&clang::Entity<'_>, &PathBuf, Vec<csick::types::Function>) -> Vec<csick::types::Function>,
>(
    mut funcs: Vec<csick::types::Function>,
    p: &PathBuf,
    f: &mut F,
) -> Vec<csick::types::Function> {
    // Browse directories recursively from starting point.
    if p.is_dir() {
        debug!("Running on folder: {:?}", p);
        // Loop through files and gather function info for them.
        match p.read_dir() {
            Ok(entries) => {
                for entry in entries {
                    let file_or_dir = match entry {
                        Ok(e) => e.path(),
                        Err(e) => {
                            error!(
                                "Failed to read entry in {}: {}\nContinuing parse...",
                                p.display(),
                                e
                            );
                            continue;
                        }
                    };
                    funcs = parse_common(funcs, &file_or_dir, f);
                }
            }
            Err(e) => {
                error!(
                    "Unable to read directory {}: {}\nContinuing parse...",
                    p.display(),
                    e
                );
            }
        }
        return funcs;
    } else if p.extension().is_some_and(|ext| ext == "h" || ext == "cpp")
        && p.file_prefix().is_some_and(|name| name != "csick")
    {
        debug!("Parsing for file: {:?}", p);

        // Setup Clang and get TranslationUnit
        let clang = clang::Clang::new().expect("Unable to instantiate clang.");
        let index = clang::Index::new(&clang, true, false);
        let mut parser = index.parser(p);
        parser.skip_function_bodies(false);
        let tu = match parser.arguments(&ARGS).parse() {
            Ok(tu) => tu,
            Err(e) => {
                error!("Parse error: {:?}\n\tIn file {:?}", e, p);
                return funcs;
            }
        };

        // For each function
        let root = tu.get_entity();
        funcs = f(&root, p, funcs);
    } else {
        debug!("Ignoring file: {:?}", p);
    }
    funcs
}

fn should_parse(cursor: clang::Entity<'_>) -> bool {
    let kind = cursor.get_kind();
    let is_function = kind == clang::EntityKind::FunctionDecl;
    let is_method = kind == clang::EntityKind::Method;
    let is_constructor = kind == clang::EntityKind::Constructor;
    let is_imported = cursor
        .get_location()
        .is_some_and(|loc| !loc.is_in_main_file());
    (is_function || is_method || is_constructor) && !is_imported
}

fn is_canonical(cursor: &clang::Entity<'_>) -> Option<bool> {
    let canonical_path = cursor
        .get_canonical_entity()
        .get_location()?
        .get_file_location()
        .file?
        .get_path();
    let cursor_path = cursor.get_location()?.get_file_location().file?.get_path();
    Some(canonical_path == cursor_path)
}

pub fn rust_sanitize(name: &str) -> String {
    let name = name.replace("::", "_");
    if RUST_RESERVED.contains(&name.as_str()) {
        format!("csick_{}", name)
    } else {
        name
    }
}

fn get_system_include_paths() -> Vec<String> {
    let output = Command::new("clang++")
        .args(["-E", "-x", "c++", "-", "-v"])
        .stdin(std::process::Stdio::null())
        .output()
        .ok();

    let mut paths = Vec::new();
    if let Some(out) = output {
        let stderr = String::from_utf8_lossy(&out.stderr);
        let mut collect = false;
        for line in stderr.lines() {
            if line.contains("#include <...> search starts here:") {
                collect = true;
                continue;
            }
            if line.contains("End of search list.") {
                break;
            }
            if collect {
                paths.push(format!("-I{}", line.trim()));
            }
        }
    }
    paths
}

static ALLOWED_ALPHABET: LazyLock<Vec<char>> =
    LazyLock::new(|| ('a'..='z').chain('0'..='9').collect());

const RUST_RESERVED: &[&str] = &[
    // Crate entry-point names
    "main",
    "lib", // Strict keywords
    "_",
    "as",
    "async",
    "await",
    "break",
    "const",
    "continue",
    "crate",
    "dyn",
    "else",
    "enum",
    "extern",
    "false",
    "fn",
    "for",
    "if",
    "impl",
    "in",
    "let",
    "loop",
    "match",
    "mod",
    "move",
    "mut",
    "pub",
    "ref",
    "return",
    "self",
    "Self",
    "static",
    "struct",
    "super",
    "trait",
    "true",
    "type",
    "unsafe",
    "use",
    "where",
    "while", // Reserved keywords
    "abstract",
    "become",
    "box",
    "do",
    "final",
    "gen",
    "macro",
    "override",
    "priv",
    "try",
    "typeof",
    "unsized",
    "virtual",
    "yield", // Weak keywords
    "'static",
    "macro_rules",
    "raw",
    "safe",
    "union",
];

const ARGS: LazyLock<Vec<String>> = LazyLock::new(|| {
    let mut args = vec![
        "-x".to_string(),
        "c++".to_string(),
        "-std=c++17".to_string(),
    ];
    let system_args = get_system_include_paths();
    args.extend(system_args);
    args
});

/// # parse_csick
/// Parses a file or directory for contained csick annotated functions.
///
/// ## Arguments
/// * `p` - PathBuf of the folder or file to parse
///
/// ## Return
/// Vec<gener::csick::types::Function> - Parsed functions from the read files.
fn parse_csick(mut funcs: Vec<csick::types::Function>, p: &PathBuf, additional_mappings: &HashMap<String, String>) -> Vec<csick::types::Function> {
    let map = if additional_mappings.is_empty() { None } else { Some(additional_mappings) };
    funcs = parse_common(funcs, p, &mut |root, p, mut f| {
        root.visit_children(|cursor: clang::Entity<'_>, _parent| {
            // Skip non-functions and/or imported content.
            if !should_parse(cursor) || !is_canonical(&cursor).unwrap_or(true) {
                return clang::EntityVisitResult::Recurse;
            }

            let cpp_name = match cursor.get_name() {
                Some(n) => n,
                None => return clang::EntityVisitResult::Recurse,
            };

            // Things to track through the function
            let mut found_csick: bool = false;
            let mut found_csickd: bool = false;
            let mut cpp_params: Vec<csick::types::GenericParameter> = Vec::new();
            let mut existing_id: Option<String> = None;
            let ret = cursor.get_result_type().unwrap().get_display_name();

            // Visit each child
            cursor.visit_children(|child, _child_parent| {
                let child_kind = child.get_kind();
                let child_name = child.get_display_name().unwrap_or_default();

                // Check if child is an annotation and CSICK
                if child_kind == clang::EntityKind::AnnotateAttr {
                    if child_name == "CSICK" {
                        debug!("Found CSICK function: {:?}", cpp_name);
                        found_csick = true;
                    } else if child_name == "CSICKD" {
                        debug!("Found CSICKD function: {:?}", cpp_name);
                        found_csickd = true;
                    } else if child_name.starts_with("CSK_") {
                        if let Some(id) = child_name.strip_prefix("CSK_") {
                            existing_id = Some(String::from(id));
                        }
                    }
                }
                // Otherwise, if it is a parameter add it
                else if child_kind == clang::EntityKind::ParmDecl {
                    info!(
                        "    -> Found parameter: {:?}, ${:?}",
                        child_name,
                        child.get_type().map(|t| t.get_display_name()).unwrap()
                    );
                    cpp_params.push(csick::types::GenericParameter {
                        name: child_name,
                        r#type: child
                            .get_type()
                            .map(|t| t.get_display_name())
                            .unwrap_or(String::from(csick::mapping::UNKNOWN_TYPE)),
                    });
                }
                clang::EntityVisitResult::Continue
            });
            if !found_csick && !found_csickd {
                return clang::EntityVisitResult::Recurse;
            }
            let unique_id = if found_csick {
                format!("_{}", nanoid::nanoid!(12, &ALLOWED_ALPHABET))
            } else {
                if let Some(id) = existing_id {
                    id
                } else {
                    warn!(
                        "Unable to find existing id for CSICKD function: {}.",
                        &cpp_name
                    );
                    format!("_{}", nanoid::nanoid!(12, &ALLOWED_ALPHABET))
                }
            };
            let cpp_name = cursor
                .get_semantic_parent()
                .filter(|p| {
                    matches!(
                        p.get_kind(),
                        clang::EntityKind::ClassDecl | clang::EntityKind::StructDecl
                    )
                })
                .and_then(|p| p.get_name())
                .map(|prefix| format!("{}::{}", prefix, cpp_name))
                .unwrap_or(cpp_name);
            let rust_params =
                cpp_params
                    .clone()
                    .into_iter()
                    .fold(Vec::new(), |mut acc, mut func| {
                        func.r#type = csick::mapping::cpp_to_rust(&func.r#type, map);
                        acc.push(func);
                        acc
                    });
            f.push(csick::types::Function {
                annotation: if found_csick {
                    csick::CSICKAnnotation::CSICK
                } else {
                    // None not possible here
                    csick::CSICKAnnotation::CSICKD
                },
                unique_id,
                declaration_file: p.clone(),
                rust_name: rust_sanitize(&cpp_name),
                cpp_name,
                rust_params,
                rust_return_type: csick::mapping::cpp_to_rust(&ret, map),
                cpp_params,
                cpp_return_type: ret,
                existing_cpp_body: None, // to be filled in parse_bodies
            });
            clang::EntityVisitResult::Recurse
        });
        f
    });
    funcs
}

/// # parse_bodies
/// Performs a pass to collect bodies, then adds them to the function with matching name and file in funcs.
///
/// ## Arguments
/// * `p` - PathBuf of the folder or file to parse
/// * `funcs` - Functions to be modified
///
/// ## Return
/// Vec<csick::types::Function> - Parsed functions from the read files.
fn parse_bodies(
    mut funcs: Vec<csick::types::Function>,
    p: &PathBuf,
) -> Vec<csick::types::Function> {
    funcs = parse_common(funcs, p, &mut |root, _, mut f| {
        root.visit_children(|cursor: clang::Entity<'_>, _parent| {
            // Skip non-functions and/or imported content.
            if !should_parse(cursor) {
                return clang::EntityVisitResult::Recurse;
            }

            let name = match cursor.get_name() {
                Some(n) => n,
                None => return clang::EntityVisitResult::Recurse,
            };

            // Things to track through the function
            let mut found_csick: bool = false;

            // Visit each aspect
            cursor.visit_children(|child, _child_parent| {
                // Get info about child
                let child_kind = child.get_kind();
                let child_name = child.get_display_name().unwrap_or_default();

                // Check if child is an annotation and CSICK
                if child_kind == clang::EntityKind::AnnotateAttr {
                    if child_name == "CSICK" {
                        debug!("Found CSICK function: {:?}", name);
                        found_csick = true;
                    }
                }
                clang::EntityVisitResult::Continue
            });
            if found_csick {
                let cpp_name = cursor
                    .get_semantic_parent()
                    .filter(|p| {
                        matches!(
                            p.get_kind(),
                            clang::EntityKind::ClassDecl | clang::EntityKind::StructDecl
                        )
                    })
                    .and_then(|p| p.get_name())
                    .map(|prefix| format!("{}::{}", prefix, name))
                    .unwrap_or_else(|| name.clone());
                if let Some(existing) = try_get_existing_definition(cursor)
                    && let Some(add_to) = f.iter_mut().find(|f| {
                        f.cpp_name == cpp_name && f.declaration_file == existing.canon_path
                    })
                {
                    add_to.existing_cpp_body = Some(existing);
                }
            } else {
                return clang::EntityVisitResult::Recurse;
            }
            clang::EntityVisitResult::Recurse
        });
        f
    });
    funcs
}

fn try_get_existing_definition(
    cursor: clang::Entity<'_>,
) -> Option<csick::types::ExistingDefinition> {
    let def = cursor.get_definition()?;
    let def_path = def.get_location()?.get_file_location().file?.get_path();

    // Check if this is all in the file it came from.
    let is_inline = is_canonical(&cursor).unwrap_or(false);

    let file_content = fs::read_to_string(&def_path).ok()?;

    if is_inline {
        // Only drain the body so the declaration remains for modify_cpp_declaration
        let mut body_range = None;
        def.visit_children(|child, _| {
            if child.get_kind() == clang::EntityKind::CompoundStmt {
                body_range = child.get_range();
                return clang::EntityVisitResult::Break;
            }
            clang::EntityVisitResult::Continue
        });
        let body_range = body_range?;
        let start = body_range.get_start().get_spelling_location();
        let end = body_range.get_end().get_spelling_location();
        let byte_range = (start.offset as usize)..(end.offset as usize + 1);
        let text = file_content.get(byte_range.clone())?.to_string();
        Some(csick::types::ExistingDefinition {
            canon_path: def_path.clone(),
            def_path,
            text,
        })
    } else {
        let canonical_path = cursor
            .get_canonical_entity()
            .get_location()?
            .get_file_location()
            .file?
            .get_path();

        // Drain body and declaration since main declaration should be in another file.
        let range = cursor.get_range()?;

        // Use get_file_location() so offsets are in def_path, not in a macro's header.
        let start = range.get_start().get_file_location();
        let end = range.get_end().get_file_location();
        let byte_range = (start.offset as usize)..(end.offset as usize);
        let text = String::from(file_content.get(byte_range.clone())?);
        Some(csick::types::ExistingDefinition {
            canon_path: canonical_path,
            def_path,
            text,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::csick::setup::CSICK_H;

    fn make_test_dir(name: &str, files: &[(&str, &str)]) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(name);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("csick.h"), CSICK_H).unwrap();
        for (fname, content) in files {
            std::fs::write(dir.join(fname), content).unwrap();
        }
        dir
    }

    #[test]
    fn sanitize_normal_name() {
        assert_eq!(rust_sanitize("myFunction"), "myFunction");
        assert_eq!(rust_sanitize("add"), "add");
    }

    #[test]
    fn sanitize_reserved_words() {
        assert_eq!(rust_sanitize("main"), "csick_main");
        assert_eq!(rust_sanitize("mod"), "csick_mod");
        assert_eq!(rust_sanitize("fn"), "csick_fn");
        assert_eq!(rust_sanitize("self"), "csick_self");
        assert_eq!(rust_sanitize("lib"), "csick_lib");
    }

    #[test]
    fn sanitize_namespace_separator() {
        assert_eq!(rust_sanitize("MyClass::myMethod"), "MyClass_myMethod");
    }

    #[test]
    fn empty_dir_returns_nothing() {
        let dir = std::env::temp_dir().join("csick_parse_empty");
        std::fs::create_dir_all(&dir).unwrap();
        // No .h/.cpp files so Clang is never instantiated; no lock needed.
        assert!(parse(&dir, &HashMap::new()).is_empty());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn skips_csick_h() {
        let dir = make_test_dir("csick_parse_skip_h", &[]);
        // csick.h is filtered out by name before Clang is instantiated.
        assert!(parse(&dir, &HashMap::new()).is_empty());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn unannotated_function_ignored() {
        let dir = make_test_dir(
            "csick_parse_unannotated",
            &[("util.h", "#include \"csick.h\"\nint helper(int x);")],
        );
        crate::csick::with_clang(|| {
            assert!(parse(&dir, &HashMap::new()).is_empty());
        });
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn finds_annotated_free_function() {
        let dir = make_test_dir(
            "csick_parse_free_fn",
            &[(
                "math.h",
                "#include \"csick.h\"\nCSICK int add(int a, int b);",
            )],
        );
        crate::csick::with_clang(|| {
            let result = parse(&dir, &HashMap::new());
            assert_eq!(result.len(), 1);
            assert_eq!(result[0].cpp_name, "add");
            assert_eq!(result[0].rust_name, "add");
            assert_eq!(result[0].cpp_return_type, "int");
            assert_eq!(result[0].rust_return_type, "i32");
            assert_eq!(result[0].cpp_params[0].name, "a");
            assert_eq!(result[0].cpp_params[0].r#type, "int");
            assert_eq!(result[0].rust_params[0].r#type, "i32");
            assert_eq!(result[0].cpp_params[1].name, "b");
            assert_eq!(result[0].rust_params[1].r#type, "i32");
            assert!(!result[0].unique_id.is_empty());
        });
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn finds_annotated_class_method() {
        let dir = make_test_dir(
            "csick_parse_method",
            &[(
                "proc.h",
                "#include \"csick.h\"\nclass Processor {\npublic:\n    CSICK float process(float input);\n};",
            )],
        );
        crate::csick::with_clang(|| {
            let result = parse(&dir, &HashMap::new());
            assert_eq!(result.len(), 1);
            assert_eq!(result[0].cpp_name, "Processor::process");
            assert_eq!(result[0].rust_name, "Processor_process");
            assert_eq!(result[0].cpp_return_type, "float");
            assert_eq!(result[0].rust_return_type, "f32");
        });
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn pointer_params_mapped_correctly() {
        let dir = make_test_dir(
            "csick_parse_ptr",
            &[(
                "buf.h",
                "#include \"csick.h\"\nCSICK void fill(float * buf, int len);",
            )],
        );
        crate::csick::with_clang(|| {
            let result = parse(&dir, &HashMap::new());
            assert_eq!(result.len(), 1);
            assert_eq!(result[0].rust_params[0].r#type, "*mut f32");
            assert_eq!(result[0].rust_params[1].r#type, "i32");
        });
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn recurses_into_subdirectories() {
        let dir = std::env::temp_dir().join("csick_parse_recurse");
        let sub = dir.join("nested");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(sub.join("csick.h"), CSICK_H).unwrap();
        std::fs::write(
            sub.join("deep.h"),
            "#include \"csick.h\"\nCSICK int deep_fn(int x);",
        )
        .unwrap();
        crate::csick::with_clang(|| {
            let result = parse(&dir, &HashMap::new());
            assert_eq!(result.len(), 1);
            assert_eq!(result[0].cpp_name, "deep_fn");
            assert_eq!(result[0].rust_name, "deep_fn");
        });
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn bodies_finds_existing_cpp_body() {
        let dir = make_test_dir(
            "csick_parse_bodies",
            &[
                ("impl.h", "#include \"csick.h\"\nCSICK int triple(int x);"),
                (
                    "impl.cpp",
                    "#include \"impl.h\"\nCSICK int triple(int x) {\n    return x * 3;\n}",
                ),
            ],
        );
        crate::csick::with_clang(|| {
            let funcs = parse(&dir, &HashMap::new());
            assert_eq!(funcs.len(), 1);
            assert!(funcs[0].existing_cpp_body.is_some());
            let body = funcs[0].existing_cpp_body.as_ref().unwrap();
            assert!(body.text.contains("x * 3"));
        });
        std::fs::remove_dir_all(&dir).ok();
    }
}
