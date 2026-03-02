use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::csick;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExistingDefinition {
    pub canon_path: PathBuf,
    pub def_path: PathBuf,
    pub text: String,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GenericParameter {
    pub name: String,   // param name
    pub r#type: String, // param type
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Function {
    #[serde(skip)]
    pub annotation: csick::CSICKAnnotation,
    pub unique_id: String,
    pub cpp_name: String,
    pub rust_name: String,
    pub declaration_file: PathBuf,
    pub cpp_params: Vec<GenericParameter>,
    pub cpp_return_type: String,
    pub rust_params: Vec<GenericParameter>,
    pub rust_return_type: String,
    #[serde(skip)]
    pub existing_cpp_body: Option<ExistingDefinition>,
}
