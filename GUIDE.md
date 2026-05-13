# Lemon 语言参考手册与 lemonc 编译器指南

---

## 目录

- [第一部分：lemonc 编译器使用指南](#第一部分lemonc-编译器使用指南)
  - [1.1 安装与构建](#11-安装与构建)
  - [1.2 基本用法](#12-基本用法)
  - [1.3 命令行选项](#13-命令行选项)
  - [1.4 编译目标](#14-编译目标)
  - [`--target c`（默认）](#target-c-默认)
  - [`--target nasm`](#target-nasm)
  - [`--target exe`](#target-exe)
  - [`--target native`](#target-native)
  - [`--target bytecode`](#target-bytecode)
- [1.5 编译流程](#15-编译流程)
  - [1.6 优化选项](#16-优化选项)
  - [1.7 跨平台支持](#17-跨平台支持)
  - [1.8 源码注解编译](#18-源码注解编译)
  - [1.9 项目构建](#19-项目构建)
- [第二部分：lemonvm JIT 运行时](#第二部分lemonvm-jit-运行时)
  - [2.1 基本用法](#21-基本用法)
  - [2.2 命令行选项](#22-命令行选项)
  - [2.3 JIT 编译原理](#23-jit-编译原理)
  - [2.4 字节码执行流程](#24-字节码执行流程)
- [第三部分：Lemon 语言参考](#第三部分lemon-语言参考)
  - [3.1 程序结构](#31-程序结构)
  - [3.2 数据类型](#32-数据类型)
  - [3.3 变量声明](#33-变量声明)
  - [3.4 运算符](#34-运算符)
  - [3.5 控制流](#35-控制流)
  - [3.6 类与对象](#36-类与对象)
  - [3.7 接口](#37-接口)
  - [3.8 泛型](#38-泛型)
  - [3.9 异常处理](#39-异常处理)
  - [3.10 内建类型与方法](#310-内建类型与方法)
  - [3.11 注解](#311-注解)
  - [3.12 其他特性](#312-其他特性)
- [第四部分：完整示例](#第四部分完整示例)
  - [示例 1：Hello World](#示例-1hello-world)
  - [示例 2：继承与多态](#示例-2继承与多态)
  - [示例 3：接口实现](#示例-3接口实现)
  - [示例 4：异常处理](#示例-4异常处理)
  - [示例 5：集合类型](#示例-5集合类型)
  - [示例 6：泛型](#示例-6泛型)
  - [示例 7：原生代码生成](#示例-7原生代码生成-target-native)
  - [示例 8：带字段和构造函数的类](#示例-8带字段和构造函数的类-target-native)

---

# 第一部分：lemonc 编译器使用指南

## 1.1 安装与构建

### 前置条件

- **Rust 工具链**：需安装 Rust 1.70+（推荐使用 `rustup` 安装）
- **C 编译器**（仅 `--target exe` 需要）：
  - Windows：GCC (MinGW) / Clang / MSVC
  - Linux：GCC / Clang
  - macOS：Clang (Xcode Command Line Tools)
- **NASM**（仅 `--target nasm` 需要）：安装 NASM 汇编器

### 构建 lemonc

```bash
# 克隆项目
git clone <repo-url>
cd lemonc

# Release 构建（推荐）
cargo build --release

# 构建产物位于：
# target/release/lemonc          (Linux/macOS)
# target/x86_64-pc-windows-gnu/release/lemonc.exe  (Windows GNU)
```

## 1.2 基本用法

```bash
# 编译 Lemon 源码为 C 代码（默认）
lemonc hello.lm

# 编译为可执行文件
lemonc hello.lm --target exe

# 指定输出文件名
lemonc hello.lm -o output.c

# 仅词法分析
lemonc hello.lm --lex-only

# 仅语法分析
lemonc hello.lm --parse-only
```

## 1.3 命令行选项

| 选项 | 说明 |
|------|------|
| `-o <file>` | 指定输出文件路径 |
| `-O<level>` | 优化级别（0-3），默认为 `-O1` |
| `--lex-only` | 仅运行词法分析，输出 Token 列表 |
| `--parse-only` | 仅运行词法分析 + 语法分析，输出 AST |
| `--target <tgt>` | 编译目标：`c`、`nasm`、`exe`、`native`、`bytecode` |
| `--keep-intermediate` | 保留中间文件（.c / .asm / .obj），`--target exe` 和 `--target native` 时有效 |

## 1.4 编译目标

### `--target c`（默认）

生成 C 源代码文件。输出 `.c` 文件，可使用任何 C 编译器编译为可执行文件。

```bash
lemonc hello.lm                    # 生成 hello.c
gcc hello.c -o hello               # 手动编译为可执行文件
```

### `--target nasm`

生成 NASM x86-64 汇编代码。输出 `.asm` 文件。

```bash
lemonc hello.lm --target nasm      # 生成 hello.asm
nasm -f win64 -o hello.o hello.asm  # 汇编为目标文件
gcc hello.o -o hello                # 链接为可执行文件
```

### `--target exe`

直接编译为可执行文件。自动检测操作系统和可用的 C 编译器，一步完成编译。

```bash
lemonc hello.lm --target exe       # 直接生成可执行文件
lemonc hello.lm --target exe --keep-intermediate  # 保留中间 .c 文件
```

### `--target native`

直接生成原生 x86-64 机器码可执行文件，无需生成 C 代码或汇编代码作为中间步骤。编译器直接输出 COFF 目标文件（Windows）并通过系统链接器链接为可执行文件。

```bash
lemonc hello.lm --target native     # 直接生成原生可执行文件
lemonc hello.lm --target native --keep-intermediate  # 保留 .obj 目标文件
```

**`--target native` 特性：**

- 直接生成 x86-64 机器码，不经过 C 语言中间层
- 使用 Windows x64 调用约定（`rcx`, `rdx`, `r8`, `r9` 寄存器传参）
- 输出 COFF64 目标文件格式（Windows）
- 自动调用系统链接器（GCC/MinGW）生成可执行文件
- 支持类、对象、方法调用、方法重载、字段访问等完整 OOP 特性

**`--target native` 当前支持的功能：**

| 功能 | 支持状态 |
|------|---------|
| 基本类型（int, String 等） | ✅ |
| 函数定义与调用 | ✅ |
| 类定义与实例化 | ✅ |
| 构造函数（默认/自定义） | ✅ |
| 实例方法调用 | ✅ |
| 静态方法调用 | ✅ |
| 方法重载 | ✅ |
| 字段访问与赋值 | ✅ |
| `this` 关键字 | ✅ |
| 控制流（if/for/while） | ✅ |
| 字符串字面量 | ✅ |
| 外部函数调用（如 printf） | ✅ |

**示例：**

```bash
# 编译方法重载示例
lemonc overload_test.lm --target native -o overload_test.exe

# 编译带构造函数和字段的类
lemonc native_class_test.lm --target native -o class_test.exe
```

### `--target bytecode`

编译为 Lemon 字节码文件（`.lmb`）。字节码是一种中间表示形式，可由 `lemonvm` JIT 运行时加载并执行。

```bash
lemonc hello.lm --target bytecode     # 生成 hello.lmb
lemonvm hello.lmb                      # 使用 JIT 运行时执行
```

**`--target bytecode` 特性：**

- 生成平台无关的字节码，可在任何支持 `lemonvm` 的平台上运行
- 保留完整的类型信息和调试符号
- 支持 JIT 编译加速执行
- 适合快速迭代开发和脚本执行场景

**字节码文件格式（.lmb）：**

| 部分 | 说明 |
|------|------|
| Magic | `LMB\0` (4 bytes) |
| Version | u32 版本号 |
| String Pool | 字符串常量池 |
| Function Table | 函数字节码 |
| Class Table | 类元数据 |
| Globals | 全局变量名 |
| Entry Point | 主函数索引 |

## 1.5 编译流程

lemonc 的编译管线包含以下阶段：

```
源码 (.lm)
  │
  ▼
[1/5] 词法分析 (Lexer)        → Token 流
  │
  ▼
[2/5] 语法分析 (Parser)       → AST
  │
  ▼
[2.5/5] 语义分析 (Semantic)   → 类型检查、继承验证、接口检查
  │
  ▼
[3/5] IR 生成 (IR Gen)        → 中间表示
  │
  ▼
[4/5] 优化 (Optimizer)        → 常量折叠、常量传播、死代码消除
  │
  ▼
[5/5] 代码生成 (CodeGen)      → C 代码 / NASM 汇编 / 原生机器码
  │
  ▼
[6/6] 编译为可执行文件 (仅 --target exe / --target native)
```

## 1.6 优化选项

| 级别 | 说明 |
|------|------|
| `-O0` | 不优化 |
| `-O1` | 默认优化级别，启用常量折叠、常量传播、死代码消除 |
| `-O2` | 同 `-O1`（当前版本） |
| `-O3` | 同 `-O1`（当前版本） |

优化器当前支持以下优化：

- **常量折叠**：编译期计算常量表达式（如 `3 + 5` → `8`）
- **常量传播**：追踪变量常量值并替换使用处
- **死代码消除**：移除不可达代码和无副作用表达式
- **代数化简**：`0 + x` → `x`、`x * 1` → `x`
- **短路优化**：`false && x` → `false`、`true || x` → `true`

## 1.7 跨平台支持

`--target exe` 模式自动检测操作系统并选择合适的编译器：

| 操作系统 | 可执行文件格式 | 编译器搜索顺序 |
|----------|---------------|---------------|
| Windows | `.exe` | GCC → Clang → MSVC (cl.exe) → CC |
| Linux | ELF | GCC / CC → Clang |
| macOS | Mach-O | Clang → GCC → CC |
| FreeBSD | ELF | GCC / CC → Clang |
| OpenBSD | ELF | GCC / CC → Clang |

### `--target native` 跨平台说明

`--target native` 当前在 Windows 上通过 MinGW GCC 链接器生成 PE 可执行文件。其他平台的支持正在开发中。

| 操作系统 | 目标文件格式 | 链接器 | 状态 |
|----------|-------------|--------|------|
| Windows | COFF64 (PE) | GCC/MinGW | ✅ 已支持 |
| Linux | ELF64 | GCC/Clang | 🚧 开发中 |
| macOS | Mach-O64 | Clang | 🚧 开发中 |

### `--target bytecode` 跨平台说明

`--target bytecode` 生成的 `.lmb` 文件是平台无关的字节码，可在任何支持 `lemonvm` 的平台上运行。

| 操作系统 | lemonvm 状态 |
|----------|-------------|
| Windows | ✅ 已支持 |
| Linux | ✅ 已支持（需自行编译）|
| macOS | ✅ 已支持（需自行编译）|

---

# 第二部分：lemonvm JIT 运行时

`lemonvm` 是 Lemon 语言的 JIT（即时编译）运行时，负责加载并执行 `.lmb` 字节码文件。它结合了解释执行和 JIT 编译两种模式，在保持启动速度的同时，通过热点代码的 JIT 编译获得接近原生代码的执行性能。

## 2.1 基本用法

```bash
# 执行字节码文件
lemonvm hello.lmb

# 启用调试模式（打印字节码）
lemonvm hello.lmb --debug

# 禁用 JIT 编译（纯解释执行）
lemonvm hello.lmb --no-jit

# 调整 JIT 编译阈值
lemonvm hello.lmb --jit-threshold 50
```

## 2.2 命令行选项

| 选项 | 说明 |
|------|------|
| `--debug` | 打印字节码指令和常量池信息 |
| `--jit` | 启用 JIT 编译（默认开启） |
| `--no-jit` | 禁用 JIT 编译，使用纯解释执行 |
| `--jit-threshold <n>` | 设置 JIT 编译阈值（默认 100） |

## 2.3 JIT 编译原理

lemonvm 采用**分层编译**策略：

### 执行模式

| 模式 | 说明 | 适用场景 |
|------|------|----------|
| 解释执行 | 逐条解释字节码指令 | 冷代码、执行次数少的函数 |
| JIT 编译 | 将热点函数编译为 x86-64 机器码 | 热代码、执行频繁的函数 |

### 热点检测

- 每个函数维护一个执行计数器
- 当函数执行次数超过阈值（默认 100 次）时，触发 JIT 编译
- 短函数（< 50 条指令）会在加载时预编译

### JIT 编译支持的指令

当前 JIT 编译器支持以下字节码指令：

| 指令 | 说明 |
|------|------|
| `PushConst` | 压入整数常量 |
| `PushFloat` | 压入浮点常量 |
| `PushBool` | 压入布尔值 |
| `PushNull` | 压入 null |
| `Pop` | 弹出栈顶 |
| `Dup` | 复制栈顶 |
| `Add` / `Sub` / `Mul` | 算术运算 |
| `Neg` | 取负 |
| `Eq` / `Ne` / `Lt` / `Gt` | 比较运算 |
| `And` / `Or` / `Not` | 逻辑运算 |
| `Return` | 函数返回 |

未支持的指令会回退到解释执行模式。

## 2.4 字节码执行流程

```
hello.lm
  │
  ▼
lemonc hello.lm --target bytecode
  │
  ▼
hello.lmb (字节码文件)
  │
  ▼
lemonvm hello.lmb
  │
  ├── [加载] 读取 .lmb 文件，构建 BytecodeModule
  │
  ├── [预编译] 短函数直接 JIT 编译
  │
  ├── [解释执行] 从入口函数开始逐条执行
  │
  ├── [热点检测] 记录函数执行次数
  │
  └── [JIT 编译] 热点函数编译为 x86-64 机器码
       │
       └── 后续调用直接执行机器码
```

### 完整示例

```bash
# 1. 编写 Lemon 源码
# hello.lm:
#   public class App {
#       public static void main(String[] args) {
#           int a = 10;
#           int b = 20;
#           printf("%d + %d = %d\n", a, b, a + b);
#       }
#   }

# 2. 编译为字节码
lemonc hello.lm --target bytecode

# 3. 使用 JIT 运行时执行
lemonvm hello.lmb
# 输出：10 + 20 = 30

# 4. 查看字节码详情
lemonvm hello.lmb --debug
```

---

## 1.8 源码注解编译

Lemon 支持在源码中通过注释添加编译注解，编译器会自动读取这些注解并选择对应的编译目标。

### 注解格式

```
// @compile key=value
```

### 支持的注解

| 注解 | 说明 | 示例 |
|------|------|------|
| `target` | 编译目标 | `// @compile target=bytecode` |
| `output` | 输出文件名 | `// @compile output=myapp.exe` |
| `optimize` | 优化级别 | `// @compile optimize=2` |
| `dep` | 依赖模块 | `// @compile dep=math,io` |
| `feature` | 启用特性 | `// @compile feature=gc` |
| `entry` | 标记入口 | `// @compile entry=true` |

### 使用示例

```
// @compile target=bytecode
// @compile optimize=2
// @compile output=calculator.lmb

package main;

public class Calculator {
    public static int add(int a, int b) {
        return a + b;
    }
}

public class App {
    public static void main(String[] args) {
        int result = Calculator.add(10, 20);
        printf("Result: %d\n", result);
    }
}
```

编译时无需指定 `--target`：

```bash
lemonc calculator.lm
# 自动检测注解，编译为 calculator.lmb
```

### 注解优先级

1. 命令行 `--target` 参数（最高优先级）
2. 源码中的 `@compile target` 注解
3. 默认值 `c`（最低优先级）

---

## 1.9 项目构建

Lemon 支持项目级构建，自动扫描项目目录中的所有 `.lm` 文件，根据注解和依赖关系进行编译。

### 项目结构

```
my_project/
├── src/
│   ├── main.lm          # 入口文件
│   ├── math/
│   │   └── utils.lm     # 数学工具模块
│   └── io/
│       └── file.lm      # 文件操作模块
└── test/
    └── test_math.lm     # 测试文件
```

### 构建命令

```bash
# 构建当前目录的项目
lemonc --build

# 构建指定目录的项目
lemonc --build ./my_project
```

### 项目构建特性

- **自动发现**：递归扫描所有 `.lm` 文件
- **依赖解析**：分析 `import` 语句和 `@compile dep` 注解
- **拓扑排序**：按依赖顺序编译
- **分类处理**：
  - 库文件（`target=library`）优先编译
  - 入口文件（`entry=true`）最后编译
  - 测试文件单独识别

### 多模块项目示例

```
// src/math/utils.lm
// @compile target=library

package math;

public class Utils {
    public static int factorial(int n) {
        if (n <= 1) return 1;
        return n * factorial(n - 1);
    }
}
```

```
// src/main.lm
// @compile target=exe
// @compile dep=math

package main;

import math.Utils;

public class App {
    public static void main(String[] args) {
        int result = Utils.factorial(5);
        printf("5! = %d\n", result);
    }
}
```

构建项目：

```bash
lemonc --build
# 输出：
# Scanning project at '.'...
# Found 2 source files
#   Entry points: 1
#   Libraries: 1
#   Tests: 0
#
# Compiling: src/math/utils.lm
#   Target: Library
#   Optimize: -O1
#
# Compiling: src/main.lm
#   Target: Exe
#   Optimize: -O1
#
# Build successful!
```

---

# 第三部分：Lemon 语言参考

## 3.1 程序结构

Lemon 程序由包声明、导入声明和类型声明组成。

```
package main;

import std.io;

class MyClass {
    // ...
}

interface MyInterface {
    // ...
}
```

### 包声明

```
package <name>;
```

每个源文件开头可以声明所属包名。

### 导入声明

```
import <path>;
import <path> as <alias>;
```

### 主类（Entry Point）

Lemon 程序必须包含一个含有 `main` 方法的类作为程序入口，主类名没有限制，不必为 `App`：

```
public class MyApp {
    public static void main(String[] args) {
        // 程序入口
    }
}
```

**主类规则：**

| 规则 | 说明 |
|------|------|
| 类名 | 任意合法类名 |
| 方法名 | 必须是 `main` |
| 方法签名 | `public static void main(String[] args)` |
| 位置 | 可以在任何包中，但通常放在 `main` 包 |

编译器会自动查找含有 `public static void main(String[] args)` 方法的类作为程序入口点。如果没有找到 `main` 方法，编译会失败。

### 默认访问权限

在 Lemon 中，**所有类、方法和字段默认都是 `public` 的**。即使没有显式添加权限修饰符，它们也具有公共访问权限。

```
// 以下两种写法等价：

class MyClass {           // 默认 public
    int value;            // 默认 public
    void doSomething() {} // 默认 public
}

public class MyClass {    // 显式 public
    public int value;     // 显式 public
    public void doSomething() {} // 显式 public
}
```

**权限修饰符：**

| 修饰符 | 说明 | 默认行为 |
|--------|------|----------|
| `public` | 公共访问，任何地方可见 | ✅ 默认 |
| `private` | 私有访问，仅类内部可见 | 需显式声明 |
| `static` | 静态成员，属于类而非实例 | 需显式声明 |
| `virtual` | 虚方法，可被子类重写 | 需显式声明 |
| `override` | 重写父类方法 | 需显式声明 |
| `abstract` | 抽象类/方法 | 需显式声明 |
| `final` | 不可继承/重写 | 需显式声明 |

**注意：** 接口中的方法默认也是 `public` 的，不需要显式声明。

## 3.2 数据类型

### 原始类型

| 类型 | 说明 | C 映射 |
|------|------|--------|
| `void` | 空类型 | `void` |
| `bool` | 布尔值 | `int` |
| `byte` | 8 位无符号整数 | `uint8_t` |
| `char` | 字符 | `char` |
| `short` | 16 位整数 | `int16_t` |
| `int` | 32 位整数 | `int32_t` |
| `long` | 64 位整数 | `int64_t` |
| `float` | 32 位浮点数 | `float` |
| `double` | 64 位浮点数 | `double` |

### 引用类型

| 类型 | 说明 | C 映射 |
|------|------|--------|
| `String` | 字符串 | `const char*` |
| `Array` | 动态数组 | `LemonArray*` |
| `Map` | 哈希映射 | `LemonMap*` |
| `TypeInfo` | 类型信息 | `TypeInfo*` |
| 自定义类 | 用户定义的类 | `struct ClassName*` |

### 数组类型

```
int[]       // 整型数组
String[]    // 字符串数组
```

### 函数指针类型

```
void(int, String)    // 函数指针
```

### 泛型类型

```
List<int>           // 整型列表
Pair<int, String>   // 整型-字符串对
Map<String, int>    // 字符串到整数的映射
```

## 3.3 变量声明

```
// 带类型声明
int x = 10;
String name = "Lemon";
Greeter g = new Greeter("Hello");

// 自动类型推断
var count = 42;
var message = "Hello";
```

### 变量修饰符

```
public static int count = 0;
private String name;
final int MAX_SIZE = 100;
```

| 修饰符 | 说明 |
|--------|------|
| `public` | 公开访问 |
| `private` | 私有访问 |
| `static` | 静态成员 |
| `final` | 不可修改 |

## 3.4 运算符

### 算术运算符

| 运算符 | 说明 | 示例 |
|--------|------|------|
| `+` | 加法 | `a + b` |
| `-` | 减法 | `a - b` |
| `*` | 乘法 | `a * b` |
| `/` | 除法 | `a / b` |
| `%` | 取模 | `a % b` |

### 关系运算符

| 运算符 | 说明 | 示例 |
|--------|------|------|
| `==` | 等于 | `a == b` |
| `!=` | 不等于 | `a != b` |
| `<` | 小于 | `a < b` |
| `>` | 大于 | `a > b` |
| `<=` | 小于等于 | `a <= b` |
| `>=` | 大于等于 | `a >= b` |

### 逻辑运算符

| 运算符 | 说明 | 示例 |
|--------|------|------|
| `&&` | 逻辑与 | `a && b` |
| `\|\|` | 逻辑或 | `a \|\| b` |
| `!` | 逻辑非 | `!a` |

### 位运算符

| 运算符 | 说明 | 示例 |
|--------|------|------|
| `&` | 按位与 | `a & b` |
| `\|` | 按位或 | `a \| b` |
| `^` | 按位异或 | `a ^ b` |
| `~` | 按位取反 | `~a` |
| `<<` | 左移 | `a << 2` |
| `>>` | 右移 | `a >> 2` |

### 赋值运算符

| 运算符 | 说明 | 等价于 |
|--------|------|--------|
| `=` | 赋值 | `a = b` |
| `+=` | 加赋值 | `a = a + b` |
| `-=` | 减赋值 | `a = a - b` |
| `*=` | 乘赋值 | `a = a * b` |
| `/=` | 除赋值 | `a = a / b` |
| `%=` | 模赋值 | `a = a % b` |
| `&=` | 与赋值 | `a = a & b` |
| `\|=` | 或赋值 | `a = a \| b` |
| `^=` | 异或赋值 | `a = a ^ b` |
| `<<=` | 左移赋值 | `a = a << b` |
| `>>=` | 右移赋值 | `a = a >> b` |

### 自增/自减

| 运算符 | 说明 | 示例 |
|--------|------|------|
| `++` | 自增 | `i++` / `++i` |
| `--` | 自减 | `i--` / `--i` |

### 其他运算符

| 运算符 | 说明 | 示例 |
|--------|------|------|
| `? :` | 三元条件 | `a > b ? a : b` |
| `as` | 类型转换 | `obj as Greeter` |
| `instanceof` | 类型检查 | `obj instanceof Greeter` |
| `new` | 创建对象 | `new Greeter("hi")` |
| `delete` | 释放对象 | `delete obj` |
| `sizeof` | 获取类型大小 | `sizeof(int)` |
| `typeid` | 获取类型 ID | `typeid(obj)` |
| `->` | 指针成员访问 | `ptr->field` |
| `*` | 解引用 | `*ptr` |
| `&` | 取地址 | `&var` |

## 3.5 控制流

### if / else

```
if (condition) {
    // ...
} else if (other) {
    // ...
} else {
    // ...
}
```

### for 循环

```
for (int i = 0; i < 10; i++) {
    printf("%d\n", i);
}
```

### while 循环

```
while (condition) {
    // ...
}
```

### break 与 continue

```
for (int i = 0; i < 100; i++) {
    if (i == 5) continue;
    if (i == 10) break;
}
```

### return

```
return;
return value;
```

## 3.6 类与对象

### 类声明

```
class ClassName {
    // 字段
    private int count;
    public String name;

    // 构造函数
    public ClassName(String name) {
        this.name = name;
        this.count = 0;
    }

    // 方法
    public int getCount() {
        return this.count;
    }

    // 虚方法
    public virtual void greet() {
        printf("Hello from %s\n", this.name);
    }
}
```

### 类修饰符

| 修饰符 | 说明 |
|--------|------|
| `public` | 公开类 |
| `private` | 私有类 |
| `abstract` | 抽象类（不可实例化） |
| `final` | 不可继承 |
| `@reflectable` | 支持反射 |

### 继承

```
class Child extends Parent {
    public Child(String name) {
        super(name);  // 调用父类构造函数
    }

    @override
    public void greet() {
        super.greet();  // 调用父类方法
        printf("Child greeting\n");
    }
}
```

- 使用 `extends` 声明继承关系
- 子类构造函数中通过 `super(...)` 调用父类构造函数
- 使用 `super.method()` 调用父类方法
- 使用 `@override` 标注重写的方法

### 方法修饰符

| 修饰符 | 说明 |
|--------|------|
| `public` | 公开方法 |
| `private` | 私有方法 |
| `static` | 静态方法 |
| `virtual` | 虚方法（支持多态） |
| `@override` | 重写父类方法 |
| `final` | 不可再重写 |
| `abstract` | 抽象方法（无方法体） |

### 构造函数与析构函数

```
class Resource {
    private void* handle;

    // 构造函数（与类同名）
    public Resource(String path) {
        this.handle = fopen(path, "r");
    }

    // 析构函数
    ~Resource() {
        if (this.handle) {
            fclose(this.handle);
        }
    }
}
```

### 对象创建与销毁

```
Greeter g = new Greeter("Hello");  // 创建对象
g.sayHello();                       // 调用方法
delete g;                           // 销毁对象
```

### 类型转换

```
LoudGreeter lg = new LoudGreeter("Hi");
Greeter poly = (Greeter)lg;         // 向上转型
lg = (LoudGreeter)poly;             // 向下转型
```

### 类型检查

```
if (obj instanceof Greeter) {
    printf("It's a Greeter\n");
}
```

### 类型信息

```
TypeInfo* t = type_of(obj);
printf("Type: %s\n", t->name);
```

### 方法访问规则

Lemon 遵循 Java 风格的方法访问规范：

| 调用场景 | 语法格式 | 示例 |
|----------|----------|------|
| 调用其他类的静态方法 | `类名.方法名(参数)` | `Math.abs(-42)` |
| 调用本类的静态方法 | `类名.方法名(参数)` | `Calculator.add(1, 2)` |
| 调用实例方法 | `对象.方法名(参数)` | `obj.greet()` |
| 在类内部调用实例方法 | `this.方法名(参数)` | `this.getValue()` |
| 在类内部调用本类静态方法 | `类名.方法名(参数)` | `MyClass.helper()` |

**禁止裸方法调用**：在类内部调用方法时，不允许省略前缀。必须使用 `this.方法名()` 或 `类名.方法名()` 的格式。

```
// 错误：裸方法调用
public class Calculator {
    public static int add(int a, int b) { return a + b; }
    public static void test() {
        int result = add(1, 2);  // 编译错误！
    }
}

// 正确：使用类名前缀
public class Calculator {
    public static int add(int a, int b) { return a + b; }
    public static void test() {
        int result = Calculator.add(1, 2);  // 正确
    }
}
```

**实例方法调用**：

```
public class Person {
    private String name;

    public Person(String name) {
        this.name = name;
    }

    public void greet() {
        printf("Hello, I'm %s\n", this.name);  // this. 访问字段
    }

    public void introduce() {
        this.greet();  // this. 调用实例方法
        printf("Nice to meet you!\n");
    }
}
```

## 3.7 接口

### 接口声明

```
interface Runnable {
    void run();
}

interface Comparable {
    int compareTo(Comparable other);
}
```

### 接口继承

```
interface Readable {
    void read();
}

interface Writable {
    void write();
}

interface ReadWrite extends Readable, Writable {
    void flush();
}
```

### 实现接口

```
class App implements Runnable {
    @override
    public void run() {
        printf("Running...\n");
    }
}
```

一个类可以实现多个接口：

```
class FileProcessor implements Readable, Writable {
    @override
    public void read() { /* ... */ }

    @override
    public void write() { /* ... */ }
}
```

### 接口方法调度

Lemon 通过 itable（接口方法表）实现接口方法的动态调度。每个实现了接口的类都会生成对应的 itable 实例，在构造时自动初始化。

## 3.8 泛型

Lemon 通过单态化（Monomorphization）实现泛型。每个泛型类的具体类型参数组合都会生成一份独立的代码。

### 泛型类声明

```
class Container<T> {
    T value;

    public Container(T v) {
        this.value = v;
    }

    public T getValue() {
        return this.value;
    }
}
```

### 泛型类使用

```
Container<int> ci = new Container<int>(42);
Container<String> cs = new Container<String>("hello");

int v = ci.getValue();
String s = cs.getValue();
```

### 类型参数命名规则

泛型实例化时，类型参数会被编码到生成的 C 函数名中：

| Lemon 类型 | C 名称编码 |
|-----------|-----------|
| `List<int>` | `List_int` |
| `Pair<int, String>` | `Pair_int_String` |
| `Map<String, int>` | `Map_String_int` |

### 类型参数约束

```
class SortedList<T extends Comparable> {
    // T 必须实现 Comparable 接口
}
```

## 3.9 异常处理

Lemon 通过 `setjmp/longjmp` 机制实现异常处理。

### try / catch

```
try {
    // 可能抛出异常的代码
    throw new MyError(42, "something went wrong");
} catch (MyError e) {
    // 处理异常
    printf("Error: code=%d\n", e.code);
}
```

### try / catch / finally

```
try {
    // 可能抛出异常的代码
} catch (MyError e) {
    // 处理异常
} finally {
    // 无论是否异常都会执行
    printf("Cleanup\n");
}
```

### try / finally

```
try {
    // 代码
} finally {
    // 清理资源
}
```

### throw 语句

```
throw new MyError(code, message);
throw expression;
```

### 异常类

异常可以是任何类，推荐定义包含错误码和消息的异常类：

```
class MyError {
    int code;
    String message;

    this(int c, String m) {
        code = c;
        message = m;
    }
}
```

## 3.10 内建类型与方法

### String 方法

String 在 C 层面映射为 `const char*`，提供以下内建方法：

| 方法 | 签名 | 说明 |
|------|------|------|
| `length()` | `int String_length(const char* s)` | 返回字符串长度 |
| `toUpperCase()` | `char* String_toUpperCase(const char* s)` | 转为大写 |
| `toLowerCase()` | `char* String_toLowerCase(const char* s)` | 转为小写 |
| `equals()` | `int String_equals(const char* a, const char* b)` | 比较字符串 |
| `trim()` | `char* String_trim(const char* s)` | 去除首尾空白 |
| `substring(start, end)` | `char* String_substring(const char* s, int, int)` | 截取子串 |
| `indexOf(sub)` | `int String_indexOf(const char* s, const char* sub)` | 查找子串位置 |
| `replace(old, new)` | `char* String_replace(const char* s, const char*, const char*)` | 替换子串 |
| `intToString(v)` | `char* String_intToString(int32_t v)` | 整数转字符串 |
| `toInt()` | `int32_t String_toInt(const char* s)` | 字符串转整数 |

使用示例：

```
String msg = "Hello, Lemon!";
int len = msg.length();           // 13
String upper = msg.toUpperCase(); // "HELLO, LEMON!"
int idx = msg.indexOf("Lemon");   // 7
```

**静态方法调用**：内建类型的静态方法需要使用类名前缀调用，例如 `String.intToString(42)` 将整数转换为字符串：

```
String numStr = String.intToString(42);  // "42"
int value = String.toInt("123");         // 123
```

### Array 方法

Array 在 C 层面映射为 `LemonArray*`，提供以下内建方法：

| 方法 | 说明 |
|------|------|
| `new Array()` | 创建空数组 |
| `add(elem)` | 添加元素 |
| `get(index)` | 获取元素 |
| `set(index, elem)` | 设置元素 |
| `size()` | 获取长度 |
| `removeAt(index)` | 移除指定位置元素 |

使用示例：

```
Array<int> arr = new Array();
arr.add(10);
arr.add(20);
int first = arr.get(0);  // 10
int count = arr.size();  // 2
```

### Map 方法

Map 在 C 层面映射为 `LemonMap*`，使用 FNV-1a 哈希和开放寻址法：

| 方法 | 说明 |
|------|------|
| `new Map()` | 创建空映射 |
| `put(key, value)` | 添加键值对 |
| `get(key)` | 获取值 |
| `size()` | 获取大小 |
| `containsKey(key)` | 检查键是否存在 |
| `remove(key)` | 移除键值对 |

使用示例：

```
Map<String, int> scores = new Map();
scores.put("Alice", 95);
scores.put("Bob", 87);
int aliceScore = scores.get("Alice");  // 95
bool has = scores.containsKey("Bob");  // true
```

### 垃圾回收

Lemon 内建标记-清除（Mark-Sweep）垃圾回收框架：

| 函数 | 说明 |
|------|------|
| `gc_init()` | 初始化 GC |
| `gc_alloc(size, type_info)` | 分配 GC 管理的内存 |
| `gc_mark(obj)` | 标记对象为可达 |
| `gc_sweep()` | 回收不可达对象 |

## 3.11 注解

Lemon 支持以下注解：

| 注解 | 目标 | 说明 |
|------|------|------|
| `@override` | 方法 | 标注重写父类/接口方法 |
| `@reflectable` | 类 | 启用反射支持 |

使用示例：

```
@reflectable
public class MyClass {
    @override
    public void toString() {
        // ...
    }
}
```

## 3.12 其他特性

### 外部函数

使用 `extern` 声明外部 C 函数：

```
extern int printf(String format, ...);
extern void* malloc(int size);
extern void free(void* ptr);
```

### Lambda 表达式

```
var add = (int a, int b) -> a + b;
var greet = (String name) -> {
    printf("Hello, %s!\n", name);
};
```

### 三元表达式

```
int max = a > b ? a : b;
```

### sizeof 运算符

```
int size = sizeof(int);     // 获取 int 类型大小
```

### typeid 运算符

```
TypeInfo* t = typeid(obj);  // 获取对象的类型信息
```

---

# 第四部分：完整示例

## 4.1 Hello World

```
package main;

public class Greeter {
    private String message;

    public Greeter(String msg) {
        this.message = msg;
    }

    public virtual void sayHello() {
        printf("%s\n", this.message);
    }
}

public class HelloApp {  // 主类名可以是任意合法类名，不必为 App
    public static void main(String[] args) {
        Greeter g = new Greeter("Hello, Lemon!");
        g.sayHello();
    }
}
```

## 4.2 继承与多态

```
package main;

public class Greeter {
    private String message;

    public Greeter(String msg) {
        this.message = msg;
    }

    public virtual void sayHello() {
        printf("%s\n", this.message);
    }
}

public class LoudGreeter extends Greeter {
    public LoudGreeter(String msg) {
        super(msg.toUpperCase());
    }

    @override
    public void sayHello() {
        printf("!!! ");
        super.sayHello();
        printf(" !!!");
    }
}

public class App {  // 使用 App 作为主类名只是惯例，不是强制要求
    public static void main(String[] args) {
        Greeter g = new Greeter("Hello, Lemon!");
        g.sayHello();

        LoudGreeter lg = new LoudGreeter("Hello, World!");
        lg.sayHello();

        Greeter poly = (Greeter)lg;
        poly.sayHello();
    }
}
```

输出：

```
Hello, Lemon!
!!! HELLO, WORLD! !!!
!!! HELLO, WORLD! !!!
```

## 4.3 接口实现

```
package main;

interface Runnable {
    void run();
}

public class App implements Runnable {  // 使用 App 作为主类名只是惯例，不是强制要求
    @override
    public void run() {
        printf("App is running\n");
    }

    public static void main(String[] args) {
        App app = new App();
        app.run();
    }
}
```

## 4.4 异常处理

```
package main;

class MyError {
    int code;
    String message;

    this(int c, String m) {
        code = c;
        message = m;
    }
}

int main() {
    try {
        printf("Before throw\n");
        throw new MyError(42, "something went wrong");
        printf("After throw - should not reach here\n");
    } catch (MyError e) {
        printf("Caught MyError: code=%d\n", e.code);
    }

    try {
        printf("Try block 2\n");
        throw new MyError(99, "another error");
    } catch (MyError e) {
        printf("Caught another MyError: code=%d\n", e.code);
    } finally {
        printf("Finally block executed\n");
    }

    printf("All tests passed\n");
    return 0;
}
```

## 4.5 集合类型

```
package main;

int main() {
    Array<int> arr = new Array();
    arr.add(10);
    arr.add(20);
    arr.add(30);
    printf("Array size: %d\n", arr.size());

    Map<String, int> scores = new Map();
    scores.put("Alice", 95);
    scores.put("Bob", 87);
    printf("Alice's score: %d\n", scores.get("Alice"));

    return 0;
}
```

## 4.6 泛型

```
package main;

class Container<T> {
    T value;

    public Container(T v) {
        this.value = v;
    }

    public T getValue() {
        return this.value;
    }
}

int main() {
    Container<int> ci = new Container<int>(42);
    printf("Int value: %d\n", ci.getValue());

    Container<String> cs = new Container<String>("hello");
    printf("String value: %s\n", cs.getValue());

    return 0;
}
```

## 4.7 原生代码生成（--target native）

```
package main;

public class Calculator {
    public int add(int a, int b) {
        return a + b;
    }

    public int add(int a, int b, int c) {
        return a + b + c;
    }

    public int multiply(int a, int b) {
        return a * b;
    }
}

public class App {  // 使用 App 作为主类名只是惯例，不是强制要求
    public static void main(String[] args) {
        Calculator calc = new Calculator();
        int sum2 = calc.add(3, 5);
        int sum3 = calc.add(1, 2, 3);
        int product = calc.multiply(4, 6);
        printf("3+5=%d, 1+2+3=%d, 4*6=%d\n", sum2, sum3, product);
    }
}
```

编译与运行：

```bash
lemonc calculator.lm --target native -o calculator.exe
calculator.exe
# 输出：3+5=8, 1+2+3=6, 4*6=24
```

## 4.8 带字段和构造函数的类（--target native）

```
package main;

public class Person {
    public String name;
    public int age;

    public Person(String n, int a) {
        this.name = n;
        this.age = a;
    }

    public void greet() {
        printf("Hello, my name is %s and I am %d years old.\n", this.name, this.age);
    }
}

public class App {  // 使用 App 作为主类名只是惯例，不是强制要求
    public static void main(String[] args) {
        Person p = new Person("Alice", 25);
        p.greet();
        printf("Name: %s, Age: %d\n", p.name, p.age);
    }
}
```

编译与运行：

```bash
lemonc person.lm --target native -o person.exe
person.exe
# 输出：
# Hello, my name is Alice and I am 25 years old.
# Name: Alice, Age: 25
```

## 4.9 字节码编译与 JIT 执行

```
package main;

public class Calculator {
    public static int add(int a, int b) {
        return a + b;
    }

    public static int multiply(int a, int b) {
        return a * b;
    }
}

public class App {  // 使用 App 作为主类名只是惯例，不是强制要求
    public static void main(String[] args) {
        int x = Calculator.add(10, 20);
        int y = Calculator.multiply(5, 6);
        printf("10 + 20 = %d\n", x);
        printf("5 * 6 = %d\n", y);
    }
}
```

编译与运行：

```bash
# 1. 编译为字节码
lemonc calculator.lm --target bytecode

# 2. 使用 JIT 运行时执行
lemonvm calculator.lmb
# 输出：
# 10 + 20 = 30
# 5 * 6 = 30

# 3. 查看字节码详情
lemonvm calculator.lmb --debug

# 4. 禁用 JIT，纯解释执行
lemonvm calculator.lmb --no-jit
```

---

## 附录：编译错误与警告

### 语义分析错误

| 错误 | 说明 |
|------|------|
| `Undefined type 'X'` | 使用了未定义的类型 |
| `Undefined variable 'X'` | 使用了未定义的变量 |
| `Undefined method 'M' in class 'C'` | 调用了类中不存在的方法 |
| `Undefined class 'X'` | 使用了未定义的类名 |
| `Duplicate definition 'X'` | 重复定义 |
| `Circular inheritance detected involving 'X'` | 检测到循环继承 |
| `Invalid override of 'M' in 'C'` | 无效的方法重写 |
| `Abstract method 'M' not implemented in 'C'` | 抽象方法未实现 |
| `Class 'C' does not implement method 'M' from interface 'I'` | 接口方法未实现 |
| `Method 'M' in class 'C' does not match interface 'I' signature` | 接口方法签名不匹配 |

### 语义分析警告

| 警告 | 说明 |
|------|------|
| `Field 'F' in 'C' shadows parent field` | 字段遮蔽父类字段 |
| `Method 'M' in 'C' overrides but is not marked @override` | 建议添加 `@override` |
| `Method 'M' in 'C' is both @virtual and @override` | `@override` 已隐含 `@virtual` |
