# lemonc - Lemon Language Compiler

[![Version](https://img.shields.io/badge/version-1.0.0-blue.svg)](https://github.com/Casaer-X/lemonc)
[![License](https://img.shields.io/badge/license-MIT-green.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.95%2B-orange.svg)](https://www.rust-lang.org)

English | [简体中文](README.zh-CN.md)

> The official compiler and JIT runtime for the Lemon programming language.

`lemonc` is a complete compiler toolchain for Lemon, featuring multi-target code generation, a JIT runtime, and a project build system.

## Features

- **Complete compiler pipeline**: Lexer → Parser → Semantic Analyzer → IR → Optimizer → Code Generator
- **Multiple compilation targets**:
  - C source code (default)
  - NASM x86-64 assembly
  - Direct executable
  - Native x86-64 (COFF64)
  - Bytecode (.lmb)
- **JIT runtime** (`lemonvm`): Execute bytecode with hotspot optimization
- **Source annotations**: Auto-detect compilation target from source comments
- **Project build system**: Multi-module builds with dependency resolution

## Installation

### From Source

```bash
git clone https://github.com/Casaer-X/lemonc.git
cd lemonc
cargo build --release
```

Binaries will be at `target/release/lemonc` and `target/release/lemonvm`.

### Requirements

- Rust 1.95+
- GCC/MinGW (for C backend)

## Quick Start

### Compile a single file

```bash
# Default: generate C code
lemonc hello.lm

# Compile to executable
lemonc hello.lm --target exe -o hello.exe

# Compile to bytecode
lemonc hello.lm --target bytecode
```

### Run bytecode with JIT

```bash
lemonc hello.lm --target bytecode
lemonvm hello.lmb
```

### Project build

```bash
# Build all .lm files in current directory
lemonc --build

# Build specific project
lemonc --build ./my_project
```

## Source Annotations

Add compile annotations to your source files:

```lemon
// @compile target=bytecode
// @compile optimize=2
// @compile output=myapp.exe

package main;

public class MyApp { // 主类名可以任意命名
    public static void main(String[] args) {
        printf("Hello, World!\n");
    }
}
```

Then compile without specifying `--target`:

```bash
lemonc myapp.lm
```

## Documentation

- [User Guide](GUIDE.md) - Complete language reference and compiler usage
- [Error Reference](ERRORS_AND_WARNINGS.md) - Error messages and solutions

## VSCode Extension

For IDE support, install the [Lemon Language Support](https://github.com/Casaer-X/vscode-lemon) extension for VSCode.

## License

MIT License - see [LICENSE](LICENSE) file for details.
