use crate::csick;
use std::{
    fs,
    io::{Error, ErrorKind},
    path::Path,
};

pub fn render_mods(paths: &[Vec<String>], depth: usize) -> Vec<String> {
    let indent = "    ".repeat(depth);
    let mut lines = Vec::new();
    let mut i = 0;
    while i < paths.len() {
        let name = &paths[i][0];
        let end = i + paths[i..].iter().take_while(|p| &p[0] == name).count();
        let children: Vec<Vec<String>> = paths[i..end]
            .iter()
            .filter_map(|p| (p.len() > 1).then(|| p[1..].to_vec()))
            .collect();
        if children.is_empty() {
            lines.push(format!("{}pub mod {};", indent, name));
        } else {
            lines.push(format!("{}pub mod {} {{", indent, name));
            lines.extend(render_mods(&children, depth + 1));
            lines.push(format!("{}}}", indent));
        }
        i = end;
    }
    lines
}

pub fn cpp_param_str(params: &[csick::types::GenericParameter]) -> String {
    params
        .iter()
        .map(|p| format!("{} {}", p.r#type, p.name))
        .collect::<Vec<_>>()
        .join(", ")
}
pub fn rust_param_str(params: &[csick::types::GenericParameter]) -> String {
    params
        .iter()
        .map(|p| format!("{}: {}", p.name, p.r#type))
        .collect::<Vec<_>>()
        .join(", ")
}
fn arg_str(params: &[csick::types::GenericParameter]) -> String {
    params
        .iter()
        .map(|p| p.name.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

pub fn modify_cpp_declaration(
    header_contents: &mut String,
    function: &csick::types::Function,
) -> Result<(), Error> {
    let param_str: String = cpp_param_str(&function.cpp_params);
    let base_declaration = format!(
        "CSICK {} {}({})",
        function.cpp_return_type, function.cpp_name, param_str
    );
    let new_declaration = format!(
        "CSICKD({}) {} {}({})",
        function.unique_id, function.cpp_return_type, function.cpp_name, param_str
    );
    if let Some(start) = header_contents.find(&base_declaration) {
        header_contents.replace_range(start..start + base_declaration.len(), &new_declaration);
        Ok(())
    } else {
        Err(Error::new(
            ErrorKind::NotFound,
            format!("C++ declaration not found: {}", base_declaration),
        ))
    }
}

pub fn modify_csickd_cpp_declaration(
    header_contents: &mut String,
    old: &csick::types::Function,
    new: &csick::types::Function,
) -> Result<(), Error> {
    let old_decl = format!(
        "CSICKD({}) {} {}({})",
        old.unique_id,
        old.cpp_return_type,
        old.cpp_name,
        cpp_param_str(&old.cpp_params)
    );
    let new_decl = format!(
        "CSICKD({}) {} {}({})",
        new.unique_id,
        new.cpp_return_type,
        new.cpp_name,
        cpp_param_str(&new.cpp_params)
    );
    if let Some(start) = header_contents.find(&old_decl) {
        header_contents.replace_range(start..start + old_decl.len(), &new_decl);
        Ok(())
    } else {
        Err(Error::new(
            ErrorKind::NotFound,
            format!("CSICKD declaration not found: {}", old_decl),
        ))
    }
}

pub fn strip_existing_definition(function: &csick::types::Function) -> Result<(), Error> {
    if let Some(existing) = &function.existing_cpp_body {
        let contents = fs::read_to_string(&existing.def_path)?;
        let contents = contents.replacen(&existing.text, "", 1);
        csick::setup::write_if_changed(&existing.def_path, contents)?;
    }
    Ok(())
}
pub fn make_cpp_additional_includes(additional_includes: &[String]) -> String {
    additional_includes
        .iter()
        .map(|v| format!("#include {}", v))
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn make_extern_block(functions: &[csick::types::Function]) -> String {
    fn get_extern_signature(function: &csick::types::Function) -> String {
        let args_string: String = cpp_param_str(&function.cpp_params);
        format!(
            "{} {}({});",
            function.cpp_return_type, function.unique_id, args_string
        )
    }
    format!(
        "/*\n * CSICK Managed extern block \n */\nnamespace csick {{\nextern \"C\" {{\n{}\n}}\n}} // namespace csick",
        functions
            .iter()
            .map(|f| get_extern_signature(f))
            .collect::<Vec<_>>()
            .join("\n")
    )
}

pub fn make_cpp_definition_block(functions: &[csick::types::Function]) -> String {
    fn get_implementation_body(function: &csick::types::Function) -> String {
        format!(
            "CSICKD({}) inline {} {}({}) {{\n  return csick::{}({});\n}}",
            function.unique_id,
            function.cpp_return_type,
            function.cpp_name,
            cpp_param_str(&function.cpp_params),
            function.unique_id,
            arg_str(&function.cpp_params)
        )
    }
    format!(
        "/*\n * CSICK Managed function definitions\n */\n{}",
        functions
            .iter()
            .map(|f| get_implementation_body(f))
            .collect::<Vec<_>>()
            .join("\n")
    )
}

pub fn make_rust_bridge(functions: &[csick::types::Function], source_path: &Path) -> String {
    fn get_call(function: &csick::types::Function, source_path: &Path) -> String {
        let canonical_file = std::fs::canonicalize(&function.declaration_file)
            .unwrap_or_else(|_| function.declaration_file.clone());
        let canonical_source =
            std::fs::canonicalize(source_path).unwrap_or_else(|_| source_path.to_path_buf());
        let relative = canonical_file
            .strip_prefix(&canonical_source)
            .unwrap_or(&canonical_file);
        let stem = csick::parsing::rust_sanitize(
            relative
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or_default(),
        );
        let mut parts: Vec<String> = relative
            .parent()
            .map(|p| {
                p.iter()
                    .map(|c| c.to_str().unwrap_or_default().to_string())
                    .collect()
            })
            .unwrap_or_default();
        parts.push(stem);
        format!(
            "crate::{}::{}({})",
            parts.join("::"),
            function.rust_name,
            arg_str(&function.rust_params)
        )
    }
    fn get_bridge_signature(function: &csick::types::Function, source_path: &Path) -> String {
        format!(
            "#[unsafe(no_mangle)]\npub extern \"C\" fn {}({}) -> {} {{\n    {}\n}}",
            function.unique_id,
            rust_param_str(&function.rust_params),
            function.rust_return_type,
            get_call(function, source_path)
        )
    }
    functions
        .iter()
        .map(|f| get_bridge_signature(f, source_path))
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn modify_rust_function(
    contents: &mut String,
    old: &csick::types::Function,
    new: &csick::types::Function,
) -> Result<(), Error> {
    let new_sig = format!(
        "pub fn {}({}) -> {} {{",
        new.rust_name,
        rust_param_str(&new.rust_params),
        new.rust_return_type
    );

    let marker = format!("/* csickd:{} */\n", old.unique_id);
    if let Some(fn_start) = contents.find(&marker).map(|p| p + marker.len()) {
        let fn_end = contents[fn_start..]
            .find('\n')
            .map(|p| fn_start + p)
            .unwrap_or(contents.len());
        contents.replace_range(fn_start..fn_end, &new_sig);
        return Ok(());
    }

    // Fallback for functions written before marker support
    let old_sig = format!(
        "pub fn {}({}) -> {} {{",
        old.rust_name,
        rust_param_str(&old.rust_params),
        old.rust_return_type
    );
    if contents.contains(&old_sig) {
        *contents = contents.replacen(&old_sig, &new_sig, 1);
        Ok(())
    } else {
        Err(Error::new(
            ErrorKind::NotFound,
            format!(
                "Rust function not found by uid or signature: {}",
                old.unique_id
            ),
        ))
    }
}

pub fn make_rust_function(function: &csick::types::Function) -> String {
    let old_body_comment = if let Some(existing) = &function.existing_cpp_body {
        let commented = existing
            .text
            .lines()
            .map(|line| format!("    // {}", line))
            .collect::<Vec<_>>()
            .join("\n");
        format!("{}\n", commented)
    } else {
        String::new()
    };

    format!(
        "/* csickd:{} */\npub fn {}({}) -> {} {{\n{}    todo!()\n}}",
        function.unique_id,
        function.rust_name,
        rust_param_str(&function.rust_params),
        function.rust_return_type,
        old_body_comment
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn simple_fn() -> csick::types::Function {
        csick::types::Function {
            annotation: csick::CSICKAnnotation::CSICKD,
            unique_id: "_abc123".to_string(),
            cpp_name: "add".to_string(),
            rust_name: "add".to_string(),
            declaration_file: PathBuf::from("example.h"),
            cpp_params: vec![
                csick::types::GenericParameter {
                    name: "a".to_string(),
                    r#type: "int".to_string(),
                },
                csick::types::GenericParameter {
                    name: "b".to_string(),
                    r#type: "int".to_string(),
                },
            ],
            cpp_return_type: "int".to_string(),
            rust_params: vec![
                csick::types::GenericParameter {
                    name: "a".to_string(),
                    r#type: "i32".to_string(),
                },
                csick::types::GenericParameter {
                    name: "b".to_string(),
                    r#type: "i32".to_string(),
                },
            ],
            rust_return_type: "i32".to_string(),
            existing_cpp_body: None,
        }
    }

    #[test]
    fn extern_block_wraps_in_csick_namespace() {
        let block = make_extern_block(&[simple_fn()]);
        assert!(block.contains("namespace csick {"));
        assert!(block.contains("extern \"C\""));
        assert!(block.contains("int _abc123(int a, int b);"));
    }

    #[test]
    fn definition_block_generates_inline_wrapper() {
        let block = make_cpp_definition_block(&[simple_fn()]);
        assert!(block.contains("CSICKD(_abc123) inline int add(int a, int b)"));
        assert!(block.contains("csick::_abc123(a, b)"));
    }

    #[test]
    fn rust_bridge_generates_extern_c_fn() {
        let bridge = make_rust_bridge(&[simple_fn()], std::path::Path::new("."));
        assert!(bridge.contains("#[unsafe(no_mangle)]"));
        assert!(bridge.contains("pub extern \"C\" fn _abc123(a: i32, b: i32) -> i32"));
    }

    #[test]
    fn rust_function_generates_todo_stub() {
        let code = make_rust_function(&simple_fn());
        assert!(code.starts_with("/* csickd:_abc123 */\n"));
        assert!(code.contains("pub fn add(a: i32, b: i32) -> i32 {"));
        assert!(code.contains("todo!()"));
    }

    #[test]
    fn rust_function_includes_cpp_body_as_comment() {
        let mut f = simple_fn();
        f.existing_cpp_body = Some(csick::types::ExistingDefinition {
            canon_path: PathBuf::from("example.h"),
            def_path: PathBuf::from("example.cpp"),
            text: "{ return a + b; }".to_string(),
        });
        let code = make_rust_function(&f);
        assert!(code.contains("    // { return a + b; }"));
        assert!(code.contains("    todo!()"));
    }

    #[test]
    fn modify_cpp_declaration_replaces_csick_with_csickd() {
        let mut contents = "CSICK int add(int a, int b);".to_string();
        modify_cpp_declaration(&mut contents, &simple_fn()).unwrap();
        assert!(contents.contains("CSICKD(_abc123) int add(int a, int b)"));
        assert!(!contents.contains("CSICK int add"));
    }

    #[test]
    fn modify_cpp_declaration_errors_when_not_found() {
        let mut contents = "int add(int a, int b);".to_string();
        assert!(modify_cpp_declaration(&mut contents, &simple_fn()).is_err());
    }

    #[test]
    fn modify_csickd_declaration_updates_return_type() {
        let old = simple_fn();
        let mut new = simple_fn();
        new.cpp_return_type = "float".to_string();
        let mut contents = "CSICKD(_abc123) int add(int a, int b);".to_string();
        modify_csickd_cpp_declaration(&mut contents, &old, &new).unwrap();
        assert!(contents.contains("CSICKD(_abc123) float add(int a, int b)"));
    }

    #[test]
    fn modify_rust_function_updates_signature_via_marker() {
        let old = simple_fn();
        let mut new = simple_fn();
        new.rust_return_type = "f32".to_string();
        let mut contents =
            "/* csickd:_abc123 */\npub fn add(a: i32, b: i32) -> i32 {\n    todo!()\n}\n"
                .to_string();
        modify_rust_function(&mut contents, &old, &new).unwrap();
        assert!(contents.contains("pub fn add(a: i32, b: i32) -> f32 {"));
    }

    #[test]
    fn modify_rust_function_falls_back_to_signature_match() {
        let old = simple_fn();
        let mut new = simple_fn();
        new.rust_return_type = "f32".to_string();
        let mut contents = "pub fn add(a: i32, b: i32) -> i32 {\n    todo!()\n}\n".to_string();
        modify_rust_function(&mut contents, &old, &new).unwrap();
        assert!(contents.contains("pub fn add(a: i32, b: i32) -> f32 {"));
    }

    #[test]
    fn modify_rust_function_errors_when_not_found() {
        let old = simple_fn();
        let new = simple_fn();
        let mut contents = "pub fn other() -> i32 { todo!() }".to_string();
        assert!(modify_rust_function(&mut contents, &old, &new).is_err());
    }

    fn paths(raw: &[&[&str]]) -> Vec<Vec<String>> {
        raw.iter()
            .map(|p| p.iter().map(|s| s.to_string()).collect())
            .collect()
    }

    #[test]
    fn render_mods_flat_list() {
        let lines = render_mods(&paths(&[&["bar"], &["baz"], &["foo"]]), 0);
        assert_eq!(lines, vec!["pub mod bar;", "pub mod baz;", "pub mod foo;"]);
    }

    #[test]
    fn render_mods_single_nested() {
        let lines = render_mods(&paths(&[&["a", "b"]]), 0);
        assert_eq!(lines, vec!["pub mod a {", "    pub mod b;", "}"]);
    }

    #[test]
    fn render_mods_shared_prefix() {
        let lines = render_mods(&paths(&[&["a", "b"], &["a", "c"]]), 0);
        assert_eq!(
            lines,
            vec!["pub mod a {", "    pub mod b;", "    pub mod c;", "}"]
        );
    }

    #[test]
    fn render_mods_mixed_depth() {
        let lines = render_mods(&paths(&[&["a", "b"], &["c"]]), 0);
        assert_eq!(
            lines,
            vec!["pub mod a {", "    pub mod b;", "}", "pub mod c;"]
        );
    }

    #[test]
    fn render_mods_deep_nesting() {
        let lines = render_mods(&paths(&[&["a", "b", "c"]]), 0);
        assert_eq!(
            lines,
            vec![
                "pub mod a {",
                "    pub mod b {",
                "        pub mod c;",
                "    }",
                "}"
            ]
        );
    }

    #[test]
    fn render_mods_empty_input() {
        assert!(render_mods(&[], 0).is_empty());
    }

    #[test]
    fn additional_includes_formats_correctly() {
        let includes = vec!["<string>".to_string(), "\"myHeader.h\"".to_string()];
        assert_eq!(
            make_cpp_additional_includes(&includes),
            "#include <string>\n#include \"myHeader.h\""
        );
    }

    #[test]
    fn additional_includes_empty_is_empty_string() {
        assert_eq!(make_cpp_additional_includes(&[]), "");
    }

    #[test]
    fn strip_existing_definition_removes_body_from_file() {
        let path = std::env::temp_dir().join("csick_test_strip_body.cpp");
        let body = "{ return a + b; }";
        std::fs::write(&path, format!("int add(int a, int b) {}\n", body)).unwrap();
        let f = csick::types::Function {
            annotation: csick::CSICKAnnotation::NONE,
            unique_id: String::new(),
            cpp_name: String::new(),
            rust_name: String::new(),
            declaration_file: PathBuf::new(),
            cpp_params: vec![],
            cpp_return_type: String::new(),
            rust_params: vec![],
            rust_return_type: String::new(),
            existing_cpp_body: Some(csick::types::ExistingDefinition {
                canon_path: path.clone(),
                def_path: path.clone(),
                text: body.to_string(),
            }),
        };
        strip_existing_definition(&f).unwrap();
        assert!(!std::fs::read_to_string(&path).unwrap().contains(body));
        std::fs::remove_file(&path).ok();
    }
}
