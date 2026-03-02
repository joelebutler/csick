# 🌊 CSick 🤢

CSick helps to reduce error-prone manual replication in FFI creation.

- [🌊 CSick 🤢](#-csick-)
  - [Overview](#overview)
    - [Annotations](#annotations)
    - [Commands](#commands)
    - [Default Type Mappings](#default-type-mappings)
  - [Pre-requisites](#pre-requisites)
    - [LLVM](#llvm)
    - [Rust](#rust)
    - [CMake](#cmake)
  - [Setup](#setup)
  - [✨ \[Recommended!\] ✨ Integrate with CMake](#-recommended--integrate-with-cmake)
  - [Manual Use](#manual-use)
  - [Automatic Use](#automatic-use)
  - [Post-generation modification](#post-generation-modification)
  - [Common Issues and Fixes](#common-issues-and-fixes)
  - [Contributing](#contributing)

## Overview

### Annotations

Make annotations to automatically pull your code to rust.

```cpp
// Function to be extracted on next `go` run.
CSICK type myFunction(type param);

// Function already pulled into Rust. 
// Look for the matching samename.rs file.
CSICKD(_UNIQUEID) type myFunction(type param);
```

### Commands

For more information on the commands, use `csick --help`.

```bash
csick init - Initialize a project for use with CSICK.

csick look - See what CSICK sees without making changes.

csick go - Run a one-time analysis and generation.

csick watch - Automatically `csick go` on each relevant file save.
```

### Default Type Mappings

| C / C++ Type         | Rust Type | Notes |
|----------------------|-----------|-------|
| `int`                | `i32`     | |
| `float`              | `f32`     | |
| `double`             | `f64`     | |
| `void`               | `()`      | |
| `bool`               | `bool`    | |
| `char`               | `i8`      | Signedness is implementation-defined; may be `u8` on ARM |
| `unsigned char`      | `u8`      | |
| `short`              | `i16`     | |
| `unsigned short`     | `u16`     | |
| `unsigned int`       | `u32`     | |
| `long`               | —         | Platform-ambiguous (32-bit on Windows, 64-bit on Linux/macOS); set manually via `additional_mappings` |
| `unsigned long`      | —         | Platform-ambiguous (32-bit on Windows, 64-bit on Linux/macOS); set manually via `additional_mappings` |
| `long long`          | `i64`     | |
| `unsigned long long` | `u64`     | |

Types not listed here can be added via `csick.json`'s `additional_mappings` field.

## Pre-requisites

### LLVM

   CSick uses libclang at runtime to parse C++ headers. It must be installed on any machine running CSICK.

   **MacOS**

   Install LLVM via Homebrew:
   ```bash
   brew install llvm
   ```

   **Linux**
   ```bash
   sudo apt install libclang-dev
   ```

   **Windows**
   Download and install the LLVM release from https://releases.llvm.org

### Rust

   Follow the instructions on Rust's site for installation.

   https://rust-lang.org/tools/install/

### CMake 

   Only needed if using the CMake Integration.
   
   > https://cmake.org 3.19+ 

## Setup

1. From your project's root folder initialize CSICK, identifying your source folder if needed. By default, `./src` is used.
   > The [CMake integration](#-recommended--integrate-with-cmake) is included by default. If you do not wish to use CMake, pass `--no-cmake`.
   ```bash
   csick init [SOURCE_PATH] [--no-cmake]
   ```
2. Two files will be generated, `csick.json` at the root and `csick.h` at the source folder path provided.

   `csick.json` lets you control how CSICK operates.

   The default structure looks like:

   ```json
   {
      "source_path": "./src", // Or other path to C++ code.
      "crate_name": "parent_dir", // Must match crate name, used by CMakeLists.txt.
      "csick_h_path": "./src/csick.h", // Path for C++ CSICK header.
      "additional_includes": [
         // ... extra headers to include for CSICK
         // e.g. <string>, "myHeader.h"
      ],
      "additional_mappings": {
         // e.g. "std::string": "std::ffi::c_string"
      },
      "sick_functions": [
         // ... generated and managed by CSICK.
      ]
   }
   ```

3. Include `CSICK.h` in the file containing a function to be bridged.

   ```cpp
   #include "csick.h"
   ```

4. First, define the C++ equivalent function in a header file.

   ```cpp
   int sum(int a, int b);
   ```

5. Preceding the definition, insert an attribute in the following format:

   ```cpp
   CSICK int sum(int a, int b);
   ```

## ✨ [Recommended!] ✨ Integrate with CMake 

1. Your CMakeLists.txt and csick.json *must* be in the same directory.
2. Run the following command to generate a snippet for CSICK as part of your build process (cmake enabled by default).
   ```
   csick init
   ```
3. Add to your add_dependencies call (create if not existing) `${CSICK_CRATE_NAME}_rust`.
   ```cmake
   add_dependencies(my_app ${CSICK_CRATE_NAME}_rust)
   ```
4. PRIVATE Link the generated library with
   ```cmake
   target_link_libraries(my_app PRIVATE
    ${_csick_lib}
   )
   ```
5. Done! Each CMake build will re-generate and make use of the generated Rust code.

## Manual Use

1. When all of your definitions are complete, run the following command to generate all missing rust declarations and FFI code.

   ```bash
   csick go
   ```

2. ...repeat `csick go` each time changes are made to propagate them throughout the generated code.
3. Before building your C++ project, run 
   `cargo build [--release]` to generate the Rust static library.
4. Link the static library in your C++ build according to your build system's processes.

## Automatic Use

Run the following command in your terminal

   ```bash
   csick watch
   ```

CSICK will begin watching your repository for changes and run `csick go` for you on each relevant file save.

## Post-generation modification

To rename a function or update parameters, update **the original annotated C++ function**.

✅ Good: Modify original annotated function.

```cpp
CSICKD(_8h2k5c8f5cic) int upByOne(int input);
CSICKD(_8h2k5c8f5cic) int round(float input);
```

🚨 Bad: Do not modify the unique ID. If the Unique ID is modified, CSICK will consider the function **deleted** and remove all bridging. It will **NOT** alter the function to the newly provided ID.

```cpp
CSICKD(_8h2k5c8f5cic) int upByOne(int input);
CSICKD(_IDontCareMan) int upByOne(int input);
```

🚨 Bad: Do not modify the managed Rust code.

```rust
// NO MODIFICATIONS HERE
#[unsafe(no_mangle)]
pub extern "C" fn _20m7ea5oey0e(sample: f32, factor: f32) -> f32 {
    crate::PluginProcessor::distort(sample, factor)
}

```

🚨 Bad: Do not remove the Rust unique ID comment.

```rust
/* csickd:_UNIQUE_ID */
pub fn distort(sample: f32, factor: f32) -> f32 {
   // YOU CAN MODIFY HERE
}
```


🚨 Bad: Do not modify lib.rs, csick.rs or csick.h as all three files will be overwritten entirely by CSICK's managed generation.

## Common Issues and Fixes

> If you are encountering errors from a missing include, add it in csick.json's additional_includes field.

> If you receive UNKNOWN_TYPE on the Rust end, add a custom mapping in csick.json's "mappings" field.

## Contributing

For information on contributing, get started here: [CONTRIBUTING.md](./CONTRIBUTING.md).
