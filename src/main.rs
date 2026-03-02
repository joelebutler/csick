use crate::csick::cli;
use std::io::{Error, ErrorKind};
pub mod csick;

fn main() -> Result<(), Error> {
    pretty_env_logger::init();

    match clang_sys::load() {
        Ok(e) => e,
        Err(e) => {
            return Err(Error::new(
                ErrorKind::NotFound,
                format!(
                    "Failed to load libclang; install LLVM/Clang and set LIBCLANG_PATH if needed {}",
                    e
                ),
            ));
        }
    }

    let cli: cli::Cli = cli::parse();
    match cli.command {
        cli::Commands::Go { path } => {
            csick::go(path)?;
        }
        cli::Commands::Look {
            path,
            no_csick,
            no_csickd,
        } => {
            csick::look(path, !no_csick, !no_csickd)?;
        }

        cli::Commands::Watch { path } => {
            csick::watch::watch(path)?;
        }
        cli::Commands::Init {
            location,
            source_path,
            csick_h,
            cmake_path,
            no_cmake,
            force,
        } => {
            csick::setup::init(location, source_path, csick_h, cmake_path, !no_cmake, force)?;
        }
    }
    Ok(())
}
