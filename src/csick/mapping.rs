use log::error;
use std::collections::HashMap;

pub const UNKNOWN_TYPE: &str = "UNKNOWN_TYPE";

const AMBIGUOUS_TYPES: &[&str] = &["long", "unsigned long"];

const DEFAULT_MAP: &[(&str, &str)] = &[
    ("int", "i32"),
    ("float", "f32"),
    ("double", "f64"),
    ("void", "()"),
    ("bool", "bool"),
    ("char", "i8"),
    ("unsigned char", "u8"),
    ("short", "i16"),
    ("unsigned short", "u16"),
    ("unsigned int", "u32"),
    ("long long", "i64"),
    ("unsigned long long", "u64"),
];

/// # cpp_to_rust
/// Converts a C++ type string to its Rust equivalent.
/// Handles pointer types (`T *` → `*mut T`, `const T *` → `*const T`).
///
/// ## Arguments
/// * `t` - C++ type string (e.g. `"int"`, `"const float *"`)
/// * `map` - A custom map to be used for conversion (defaults to csick base map)
pub fn cpp_to_rust(t: &str, map: Option<&HashMap<String, String>>) -> String {
    // Handle pointer types (clang puts a space before *)
    if let Some(inner) = t.strip_suffix(" *") {
        let (is_const, base) = inner
            .strip_prefix("const ")
            .map(|b| (true, b))
            .unwrap_or((false, inner));
        let inner_rust = cpp_to_rust(base, map);
        if inner_rust == UNKNOWN_TYPE {
            return UNKNOWN_TYPE.to_string();
        }
        return if is_const {
            format!("*const {}", inner_rust)
        } else {
            format!("*mut {}", inner_rust)
        };
    }
    // Strip top-level const from value types (e.g. `const float` → `float`)
    if let Some(base) = t.strip_prefix("const ") {
        return cpp_to_rust(base, map);
    }
    if let Some(v) = map.and_then(|m| m.get(t)) {
        return v.clone();
    }
    if let Some((_, v)) = DEFAULT_MAP.iter().find(|(k, _): &&(&str, &str)| *k == t) {
        return v.to_string();
    }
    if AMBIGUOUS_TYPES.contains(&t) {
        error!(
            "Type '{}' is platform-ambiguous (32-bit on Windows, 64-bit on Linux/macOS). \
             Set the mapping in csick.json's additional_mappings according to the README.md instructions.",
            t
        );
    } else {
        error!(
            "Type '{}' has no known mapping. \
            Set the mapping in csick.json's additional_mappings according to the README.md instructions.",
            t
        );
    }
    UNKNOWN_TYPE.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cpp_to_rust_basic_types() {
        assert_eq!(cpp_to_rust("int", None), "i32");
        assert_eq!(cpp_to_rust("float", None), "f32");
        assert_eq!(cpp_to_rust("double", None), "f64");
        assert_eq!(cpp_to_rust("void", None), "()");
        assert_eq!(cpp_to_rust("bool", None), "bool");
        assert_eq!(cpp_to_rust("char", None), "i8");
        assert_eq!(cpp_to_rust("unsigned char", None), "u8");
        assert_eq!(cpp_to_rust("short", None), "i16");
        assert_eq!(cpp_to_rust("unsigned short", None), "u16");
        assert_eq!(cpp_to_rust("unsigned int", None), "u32");
        assert_eq!(cpp_to_rust("long long", None), "i64");
        assert_eq!(cpp_to_rust("unsigned long long", None), "u64");
    }

    #[test]
    fn cpp_to_rust_pointer_types() {
        assert_eq!(cpp_to_rust("int *", None), "*mut i32");
        assert_eq!(cpp_to_rust("float *", None), "*mut f32");
        assert_eq!(cpp_to_rust("const int *", None), "*const i32");
        assert_eq!(cpp_to_rust("const float *", None), "*const f32");
    }

    #[test]
    fn cpp_to_rust_const_value_strips_const() {
        assert_eq!(cpp_to_rust("const int", None), "i32");
        assert_eq!(cpp_to_rust("const double", None), "f64");
    }

    #[test]
    fn cpp_to_rust_unknown_returns_sentinel() {
        assert_eq!(cpp_to_rust("std::string", None), UNKNOWN_TYPE);
        assert_eq!(cpp_to_rust("MyClass", None), UNKNOWN_TYPE);
    }

    #[test]
    fn cpp_to_rust_ambiguous_long_returns_sentinel() {
        assert_eq!(cpp_to_rust("long", None), UNKNOWN_TYPE);
        assert_eq!(cpp_to_rust("unsigned long", None), UNKNOWN_TYPE);
    }

    #[test]
    fn custom_type_map() {
        let custom: HashMap<String, String> = [("c_long".to_string(), "i64".to_string())]
            .into_iter()
            .collect();
        assert_eq!(cpp_to_rust("c_long", Some(&custom)), "i64");
    }
}
