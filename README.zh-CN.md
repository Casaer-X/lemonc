# lemonc - Lemon 语言编译器

[![版本](https://img.shields.io/badge/version-1.0.0-blue.svg)](https://github.com/Casaer-X/lemonc)
[![许可证](https://img.shields.io/badge/license-MIT-green.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.95%2B-orange.svg)](https://www.rust-lang.org)

[English](README.md) | 简体中文

> Lemon 编程语言的官方编译器和 JIT 运行时。

`lemonc` 是 Lemon 的完整编译器工具链，支持多目标代码生成、JIT 运行时和项目构建系统。

## 功能特性

- **完整的编译器流水线**：词法分析 → 语法分析 → 语义分析 → 中间代码 → 优化器 → 代码生成器
- **多目标编译**：
  - C 源代码（默认）
  - NASM x86-64 汇编
  - 直接生成可执行文件
  - 原生 x86-64（COFF64）
  - 字节码（.lmb）
- **JIT 运行时** (`lemonvm`)：执行字节码并支持热点优化
- **源码注解**：从源码注释自动检测编译目标
- **项目构建系统**：多模块构建，自动解析依赖关系

## 安装

### 从源码编译

```bash
git clone https://github.com/Casaer-X/lemonc.git
cd lemonc
cargo build --release
```

编译完成后，二进制文件位于 `target/release/lemonc` 和 `target/release/lemonvm`。

### 系统要求

- Rust 1.95+
- GCC/MinGW（用于 C 后端）

## 快速开始

### 编译单个文件

```bash
# 默认：生成 C 代码
lemonc hello.lm

# 编译为可执行文件
lemonc hello.lm --target exe -o hello.exe

# 编译为字节码
lemonc hello.lm --target bytecode
```

### 使用 JIT 运行字节码

```bash
lemonc hello.lm --target bytecode
lemonvm hello.lmb
```

### 项目构建

```bash
# 构建当前目录下的所有 .lm 文件
lemonc --build

# 构建指定项目
lemonc --build ./my_project
```

## 源码注解

在源文件中添加编译注解：

```lemon
// @compile target=bytecode
// @compile optimize=2
// @compile output=myapp.exe

package main;

public class App {
    public static void main(String[] args) {
        printf("Hello, World!\n");
    }
}
```

然后无需指定 `--target` 即可编译：

```bash
lemonc myapp.lm
```

## 文档

- [用户指南](GUIDE.md) - 完整的语言参考和编译器使用说明
- [错误参考](ERRORS_AND_WARNINGS.md) - 错误消息和解决方案

## VSCode 扩展

如需 IDE 支持，请安装 VSCode 的 [Lemon Language Support](https://github.com/Casaer-X/vscode-lemon) 扩展。

## 许可证

MIT 许可证 - 详见 [LICENSE](LICENSE) 文件。
