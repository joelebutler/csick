use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "CSICK",
    version,
    about = "Automated scaffolding system for partial Rust implementation in existing C++ codebases.\n\nVisit https://github.com/joelebutler/csick for more information."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Initialize a repository with the CSICK essentials for use.
    #[command()]
    Init {
        /// Location to initialize csick.json
        #[arg(default_value = "./csick.json")]
        location: PathBuf,

        /// Force re-reinitialization with overwrite.
        #[arg(short = 'f', long, default_value_t = false)]
        force: bool,

        /// Path to source folder
        #[arg(default_value = "./src")]
        source_path: PathBuf,

        /// Path for csick.h [default: [SOURCE_PATH]/csick.h]
        #[arg(long)]
        csick_h: Option<PathBuf>,

        /// Remove CMake support if desired, otherwise runs regardless of initialization status.
        #[arg(short = 'm', long, default_value_t = false)]
        no_cmake: bool,

        /// Path to CMakeLists.txt [default: ./CMakeLists.txt]
        #[arg(long)]
        cmake_path: Option<PathBuf>,
    },

    /// Watch the source directory and run `go` on any non-managed .h, .cpp, or .rs change.
    #[command()]
    Watch {
        /// Path to csick.json
        #[arg(default_value = "./csick.json")]
        path: PathBuf,
    },

    /// Manually run the standard analysis & extraction, and make updates where needed.
    #[command()]
    Go {
        /// Path to csick.json
        #[arg(default_value = "./csick.json")]
        path: PathBuf,
    },

    /// Manually run analysis, but do not update.
    #[command()]
    Look {
        /// Path to csick.json
        #[arg(default_value = "./csick.json")]
        path: PathBuf,

        /// Whether to include CSICK functions in the output.
        #[arg(long, default_value_t = false)]
        no_csick: bool,

        /// Whether to include CSICKD functions in the output.
        #[arg(long, default_value_t = false)]
        no_csickd: bool,
    },
}

pub fn parse() -> Cli {
    Cli::parse()
}
