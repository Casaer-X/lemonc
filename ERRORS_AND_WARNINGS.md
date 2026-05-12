# Lemon 编译器 (lemonc) 错误与警告参考手册

本文档详细列出了 `lemonc` 编译器在编译过程中可能输出的所有错误、异常和警告信息，包括错误原因、示例和修复建议。

---

## 目录

- [编译流程概述](#编译流程概述)
- [第一阶段：词法分析错误](#第一阶段词法分析错误)
- [第二阶段：语法分析错误](#第二阶段语法分析错误)
- [第三阶段：语义分析错误](#第三阶段语义分析错误)
- [第四阶段：代码生成与链接错误](#第四阶段代码生成与链接错误)
- [警告信息](#警告信息)
- [运行时异常](#运行时异常)
- [错误输出格式](#错误输出格式)

---

## 编译流程概述

```
源码 (.lm)
  │
  ▼
[1/5] 词法分析 (Lexer)        → 词法错误
  │
  ▼
[2/5] 语法分析 (Parser)       → 语法错误
  │
  ▼
[2.5/5] 语义分析 (Semantic)   → 语义错误 / 警告
  │
  ▼
[3/5] IR 生成 (IR Gen)
  │
  ▼
[4/5] 优化 (Optimizer)
  │
  ▼
[5/5] 代码生成 (CodeGen)      → 代码生成错误
  │
  ▼
[6/6] 编译/链接为可执行文件    → 编译器/链接器错误
```

---

## 第一阶段：词法分析错误

词法分析器在将源代码字符流转换为 Token 序列时可能遇到的错误。

### L001: 未预期的字符

**错误信息**：`Unexpected character: 'X'`

**原因**：源代码中出现了 Lemon 语言不支持的字符。

**示例**：
```lemon
int x = 10 @ 5;  // @ 不是有效的运算符
```

**修复**：移除或替换为有效的运算符/符号。

---

### L002: 未闭合的字符串

**错误信息**：`Unterminated string`

**原因**：字符串字面量没有闭合的双引号。

**示例**：
```lemon
String msg = "Hello, World;  // 缺少结尾的 "
```

**修复**：添加闭合的双引号。

---

### L003: 未闭合的字符字面量

**错误信息**：`Unterminated char literal`

**原因**：字符字面量没有闭合的单引号，或包含多个字符。

**示例**：
```lemon
char c = 'a;   // 缺少结尾的 '
char c = 'ab'; // 字符字面量只能包含一个字符
```

**修复**：使用单引号包裹单个字符。

---

### L004: 无效的浮点数字面量

**错误信息**：`Invalid float literal`

**原因**：浮点数字面量格式错误，无法解析为有效的 `f64`。

**示例**：
```lemon
float f = 1.2.3;  // 多个小数点
```

**修复**：使用正确的浮点数格式，如 `3.14` 或 `2.5e10`。

---

### L005: 整数溢出

**错误信息**：`Integer too large`

**原因**：整数字面量超出了 64 位有符号整数的范围（`i64::MAX = 9,223,372,036,854,775,807`）。

**示例**：
```lemon
int x = 999999999999999999999999999999;  // 太大
```

**修复**：使用较小的整数值，或使用浮点数类型。

---

## 第二阶段：语法分析错误

语法分析器在将 Token 序列解析为抽象语法树（AST）时可能遇到的错误。

### P001: 未预期的 Token

**错误信息**：`Expected <expected_token>, found <actual_token> at line X col Y`

**原因**：当前位置期望某种 Token，但实际遇到了不同的 Token。

**常见场景**：

| 期望 | 实际 | 示例 |
|------|------|------|
| `;` | `}` | `int x = 5 }` |
| `{` | `public` | `class Foo public` |
| `)` | `;` | `if (x > 0;` |
| `Identifier` | `class` | `class class` |

**修复**：检查并修正语法结构，确保使用正确的符号和关键字顺序。

---

### P002: 未预期的标识符

**错误信息**：`Expected identifier, found <token> at line X col Y`

**原因**：期望一个标识符（变量名、类名、方法名等），但遇到了其他 Token。

**示例**：
```lemon
class 123 { }        // 类名不能是数字
public void 456() {} // 方法名不能是数字
```

**修复**：使用有效的标识符（字母或下划线开头，后跟字母、数字或下划线）。

---

### P003: 未预期的输入结束

**错误信息**：`Unexpected end of input, expected <token>`

**原因**：文件在语法结构完成之前就结束了。

**示例**：
```lemon
class Foo {
    public void bar() {
        // 文件在这里结束，缺少 }
```

**修复**：补全缺失的闭合符号（`}`、`)`、`]` 等）。

---

### P004: 未预期的输入结束（期望标识符）

**错误信息**：`Unexpected end of input, expected identifier`

**原因**：文件在期望标识符的位置结束。

**示例**：
```lemon
class  // 类名缺失，文件结束
```

**修复**：补全缺失的标识符。

---

## 第三阶段：语义分析错误

语义分析器检查 AST 的语义正确性，包括类型检查、作用域、继承关系等。

### S001: 未定义的类型

**错误信息**：`Undefined type 'X'`

**原因**：使用了未定义的类或类型。

**示例**：
```lemon
UnknownClass obj;  // UnknownClass 未定义
```

**修复**：确保类已定义，或检查类名拼写是否正确。

---

### S002: 未定义的变量

**错误信息**：`Undefined variable 'X'`

**原因**：使用了未声明的变量。

**示例**：
```lemon
int main() {
    int y = x + 1;  // x 未声明
    return 0;
}
```

**修复**：在使用变量之前先声明它。

---

### S003: 未定义的方法

**错误信息**：`Undefined method 'method_name' in class 'class_name'`

**原因**：在指定类中调用了不存在的方法。

**示例**：
```lemon
class Calculator {
    public int add(int a, int b) { return a + b; }
}

int main() {
    Calculator calc = new Calculator();
    calc.subtract(5, 3);  // subtract 方法不存在
    return 0;
}
```

**修复**：确保方法已定义，或检查方法名拼写。

---

### S004: 未定义的类

**错误信息**：`Undefined class 'X'`

**原因**：使用了未定义的类名（通常在 `new` 表达式或类型声明中）。

**示例**：
```lemon
Person p = new Person("Alice");  // Person 类未定义
```

**修复**：定义该类，或检查类名拼写。

---

### S005: 重复定义

**错误信息**：`Duplicate definition 'X'`

**原因**：在同一作用域内重复定义了同名的类、方法、字段或变量。

**示例**：
```lemon
class Foo {}
class Foo {}  // 重复定义

public class App {
    public static void main(String[] args) {}
    public static void main(String[] args) {}  // 重复定义
}
```

**修复**：重命名其中一个定义，或移除重复的定义。

---

### S006: 类型不匹配

**错误信息**：`Type mismatch: expected 'expected_type', found 'found_type'`

**原因**：赋值或表达式中的类型不兼容。

**示例**：
```lemon
int x = "hello";           // 期望 int，得到 String
String s = 42;             // 期望 String，得到 int
bool flag = 1;             // 期望 bool，得到 int
```

**修复**：确保类型匹配，或使用类型转换（如果支持）。

---

### S007: 不是类类型

**错误信息**：`'X' is not a class type`

**原因**：尝试对非类类型使用类相关的操作（如 `new`、`extends`、`.` 访问等）。

**示例**：
```lemon
int x = new int();  // int 不是类类型
```

**修复**：使用有效的类类型。

---

### S008: 循环继承

**错误信息**：`Circular inheritance detected involving 'X'`

**原因**：类的继承关系形成了循环。

**示例**：
```lemon
class A extends B {}
class B extends A {}  // 循环继承！
```

**修复**：打破循环继承链，确保继承关系是有向无环图（DAG）。

---

### S009: 缺少 @override 标记

**错误信息**：`Method 'method_name' in 'class_name' overrides but is not marked @override`

**原因**：子类方法重写了父类方法，但没有使用 `@override` 注解。

**示例**：
```lemon
class Parent {
    public virtual void sayHello() {}
}

class Child extends Parent {
    public void sayHello() {}  // 缺少 @override
}
```

**修复**：在重写的方法上添加 `@override` 注解。

---

### S010: 无效的方法重写

**错误信息**：`Invalid override of 'method_name' in 'class_name': <reason>`

**原因**：方法重写不符合规则（参数数量不同、返回类型不兼容等）。

**示例**：
```lemon
class Parent {
    public virtual void greet(String name) {}
}

class Child extends Parent {
    @override
    public void greet(int age) {}  // 参数类型不同
}
```

**修复**：确保重写方法的签名（参数类型、数量、返回类型）与父类方法一致。

---

### S011: 抽象方法未实现

**错误信息**：`Abstract method 'method_name' not implemented in 'class_name'`

**原因**：非抽象类继承了抽象类或实现了接口，但没有实现所有的抽象方法。

**示例**：
```lemon
abstract class Animal {
    public abstract void speak();
}

class Dog extends Animal {
    // 缺少 speak() 方法的实现
}
```

**修复**：实现所有抽象方法，或将类声明为抽象类。

---

### S012: 字段遮蔽

**错误信息**：`Field 'field_name' in 'class_name' shadows parent field`

**原因**：子类定义了与父类同名的字段，遮蔽了父类字段。

**示例**：
```lemon
class Parent {
    public int value;
}

class Child extends Parent {
    public int value;  // 遮蔽了 Parent.value
}
```

**修复**：重命名字段，或明确使用 `super.value` 访问父类字段。

---

### S013: 无效访问

**错误信息**：`Cannot access 'member' from class 'class_name'`

**原因**：尝试访问不可访问的成员（如私有字段/方法）。

**示例**：
```lemon
class Foo {
    private int secret;
}

class Bar {
    public void test() {
        Foo f = new Foo();
        int x = f.secret;  // secret 是私有的
    }
}
```

**修复**：将成员改为 `public`，或使用公共的 getter/setter 方法。

---

### S014: 接口方法未实现

**错误信息**：`Class 'class_name' does not implement method 'method_name' from interface 'interface_name'`

**原因**：类声明实现了某个接口，但没有实现接口中的所有方法。

**示例**：
```lemon
interface Runnable {
    void run();
}

class App implements Runnable {
    // 缺少 run() 方法的实现
}
```

**修复**：实现接口中定义的所有方法。

---

### S015: 接口方法签名不匹配

**错误信息**：`Method 'method_name' in class 'class_name' does not match interface 'interface_name' signature: <reason>`

**原因**：类实现了接口方法，但方法签名（参数类型、返回类型）与接口定义不匹配。

**示例**：
```lemon
interface Comparable {
    int compareTo(Object other);
}

class MyClass implements Comparable {
    public void compareTo(Object other) {}  // 返回类型应为 int，不是 void
}
```

**修复**：确保实现的方法签名与接口定义完全一致。

---

## 第四阶段：代码生成与链接错误

### G001: 文件读取错误

**错误信息**：`Error reading file 'filename': <io_error>`

**原因**：无法读取输入的 `.lm` 源文件。

**修复**：检查文件路径是否正确，文件是否存在且有读取权限。

---

### G002: 对象文件写入错误

**错误信息**：`Error writing object file: <io_error>`

**原因**：无法写入生成的目标文件（`.obj`）。

**修复**：检查输出目录是否有写入权限，磁盘空间是否充足。

---

### G003: 未知的目标类型

**错误信息**：`Unknown target: X`

**原因**：使用了不支持的 `--target` 参数值。

**修复**：使用有效的目标类型：`c`、`nasm`、`exe`、`native`。

---

### G004: 输出文件写入错误

**错误信息**：`Error writing output: <io_error>`

**原因**：无法写入生成的输出文件（`.c`、`.asm` 等）。

**修复**：检查输出路径和权限。

---

### G005: 中间文件写入错误

**错误信息**：`Error writing intermediate file: <io_error>`

**原因**：无法写入中间文件。

**修复**：检查临时目录权限和磁盘空间。

---

### C001: C 编译器未找到

**错误信息**：
```
Error: No C compiler found!
Please install one of the following:
  - GCC (https://gcc.gnu.org/)
  - Clang (https://llvm.org/)
  - MSVC (Visual Studio Build Tools) [Windows only]
```

**原因**：使用 `--target exe` 或 `--target c` 时需要 C 编译器，但系统中未找到。

**修复**：安装 GCC、Clang 或 MSVC，并确保它们在系统 PATH 中。

---

### C002: C 编译器运行失败

**错误信息**：`Failed to run compiler 'path': <error>`

**原因**：找到了 C 编译器，但无法执行它。

**修复**：检查编译器是否完整安装，是否有执行权限。

---

### C003: C 编译错误

**错误信息**：
```
Compiler error:
<compiler_output>
```

**原因**：生成的 C 代码存在语法错误或编译器无法编译。

**修复**：这通常是编译器本身的 bug，请报告 issue。

---

### N001: NASM 未找到

**错误信息**：`NASM not found. Install NASM to use --target nasm --target exe.`

**原因**：使用 `--target nasm` 时需要 NASM 汇编器，但系统中未找到。

**修复**：安装 NASM（https://www.nasm.us/）并添加到 PATH。

---

### N002: NASM 运行失败

**错误信息**：`Failed to run NASM: <error>`

**原因**：找到了 NASM，但无法执行它。

**修复**：检查 NASM 安装是否完整。

---

### N003: NASM 编译错误

**错误信息**：
```
NASM error:
<nasm_output>
```

**原因**：生成的汇编代码存在语法错误。

**修复**：这通常是编译器本身的 bug，请报告 issue。

---

### L001: 链接器未找到

**错误信息**：
```
Error: No linker found!
Please install a linker (link.exe on Windows, ld on Linux/macOS).
```

**原因**：使用 `--target exe` 或 `--target native` 时需要链接器，但系统中未找到。

**修复**：安装 GCC/MinGW（Windows）或 binutils（Linux/macOS）。

---

### L002: 链接器运行失败

**错误信息**：`Failed to run linker 'path': <error>`

**原因**：找到了链接器，但无法执行它。

**修复**：检查链接器是否完整安装。

---

### L003: 链接错误

**错误信息**：
```
Linker error:
<linker_output>
```

**原因**：链接过程中出错（如符号未定义、格式不兼容等）。

**修复**：检查生成的目标文件是否正确，链接器参数是否匹配。

---

## 警告信息

警告不会阻止编译，但提示潜在的问题。

### W001: 字段遮蔽

**警告信息**：`Field 'field_name' in 'class_name' shadows parent field`

**原因**：子类定义了与父类同名的字段。

**示例**：
```lemon
class Parent {
    public int value;
}

class Child extends Parent {
    public int value;  // 警告：遮蔽了 Parent.value
}
```

**建议**：重命名字段以避免混淆，或确保这是有意为之。

---

### W002: @virtual 和 @override 同时使用

**警告信息**：`Method 'method_name' in 'class_name' is both @virtual and @override, @override implies @virtual`

**原因**：方法同时标记了 `@virtual` 和 `@override`，但 `@override` 已经隐含了 `@virtual`。

**示例**：
```lemon
class Child extends Parent {
    @virtual
    @override
    public void method() {}  // @override 已经隐含 @virtual
}
```

**建议**：移除 `@virtual` 注解，只保留 `@override`。

---

### W003: 重写方法缺少 @override

**警告信息**：`Method 'method_name' in 'class_name' overrides parent method but is not marked @override`

**原因**：子类方法重写了父类方法，但没有使用 `@override` 注解。

**示例**：
```lemon
class Parent {
    public virtual void greet() {}
}

class Child extends Parent {
    public void greet() {}  // 警告：缺少 @override
}
```

**建议**：添加 `@override` 注解以提高代码可读性和安全性。

---

### W004: 字段或方法可能不存在

**警告信息**：`Class 'class_name' may not have field or method 'member_name'`

**原因**：通过变量访问字段或方法时，编译器无法确定该类是否包含该成员。

**示例**：
```lemon
class Foo {
    public int value;
}

void test(Foo obj) {
    int x = obj.unknown;  // 警告：Foo 可能没有 unknown 字段
}
```

**建议**：检查字段/方法名拼写，或确保类定义包含该成员。

---

## 运行时异常

运行时异常在程序执行期间发生，通常由生成的 C 代码中的错误引起。

### R001: 空指针解引用

**原因**：尝试访问 `null` 对象的成员。

**示例**：
```lemon
Foo obj = null;
obj.method();  // 运行时崩溃
```

**修复**：在使用对象前检查是否为 `null`。

---

### R002: 数组越界

**原因**：访问数组时索引超出范围。

**示例**：
```lemon
int[] arr = new int[5];
int x = arr[10];  // 运行时崩溃
```

**修复**：确保索引在有效范围内（`0 <= index < length`）。

---

### R003: 内存分配失败

**原因**：`new` 或 `malloc` 无法分配足够的内存。

**修复**：检查内存使用情况，避免内存泄漏。

---

### R004: 除零错误

**原因**：整数除以零。

**示例**：
```lemon
int x = 10 / 0;  // 运行时崩溃
```

**修复**：在除法前检查除数是否为零。

---

## 错误输出格式

### 命令行输出格式

```
[1/5] Lexical analysis...
  Tokenized 42 tokens

[2/5] Parsing...
  Parsed 3 declarations

[2.5/5] Semantic analysis...
  [Warning] 5:10 - Method 'greet' in 'Child' overrides parent method but is not marked @override
  Semantic analysis passed (1 warnings)

[3/5] Generating IR...
  IR generated successfully

[4/5] Optimizing...
  Optimization level: O1
  Folded 2 constants, propagated 3, removed 0 dead statements

[5/5] Code generation...
  Target: native
  Object file written to: hello.obj

[6/6] Linking...
  Linked with: gcc
  Output: hello.exe

Compilation successful!
```

### 错误输出格式

```
Parse errors:
  Expected Semicolon, found RightBrace at line 3 col 15

Semantic errors:
  Undefined variable 'x'
  Type mismatch: expected 'int', found 'String'
```

### 诊断函数格式

```rust
// 错误
[Error] line:col - message

// 警告
[Warning] line:col - message
```

---

## 快速参考表

### 按阶段分类

| 阶段 | 错误代码 | 错误信息 |
|------|---------|---------|
| 词法 | L001-L005 | 字符、字符串、数字相关错误 |
| 语法 | P001-P004 | Token、标识符、输入结束相关错误 |
| 语义 | S001-S015 | 类型、作用域、继承相关错误 |
| 生成 | G001-G005 | 文件 I/O、目标类型错误 |
| 编译 | C001-C003 | C 编译器相关错误 |
| 汇编 | N001-N003 | NASM 相关错误 |
| 链接 | L001-L003 | 链接器相关错误 |
| 警告 | W001-W004 | 潜在问题提示 |

### 按严重程度分类

| 严重程度 | 类型 | 是否阻止编译 |
|---------|------|-------------|
| 致命 | 词法错误、语法错误、语义错误 | ✅ 是 |
| 严重 | 编译/链接错误 | ✅ 是 |
| 警告 | 语义警告 | ❌ 否 |
| 提示 | 优化信息 | ❌ 否 |

---

## 附录：JIT 运行时错误与警告

### JIT 运行时错误

| 错误 | 说明 | 解决方案 |
|------|------|----------|
| `Error opening file 'X': ...` | 无法打开 .lmb 文件 | 检查文件路径是否正确 |
| `Error reading bytecode: ...` | 字节码文件格式错误 | 确保文件是有效的 .lmb 格式，重新编译 |
| `Error: Input file must have .lmb extension` | 输入文件扩展名不正确 | 使用 `--target bytecode` 编译生成 .lmb 文件 |
| `VM execution error: ...` | 运行时错误 | 检查代码逻辑，查看字节码 `--debug` |
| `JIT compilation failed for function 'X'` | JIT 编译失败 | 函数包含不支持的指令，自动回退到解释执行 |

### JIT 运行时警告

| 警告 | 说明 |
|------|------|
| `JIT compilation disabled` | 使用 `--no-jit` 禁用了 JIT |
| `Function 'X' too complex for JIT` | 函数指令数超过 JIT 编译限制 |
| `Unsupported bytecode instruction: X` | JIT 编译器不支持该指令，回退到解释执行 |

---

## 常见问题排查

### Q: 编译器报告 "Undefined variable"，但变量已声明

**可能原因**：
1. 变量声明在使用之后
2. 变量在不同的作用域中
3. 变量名拼写错误

**解决**：确保变量在使用之前声明，并检查拼写。

### Q: "Type mismatch" 但类型看起来正确

**可能原因**：
1. 方法返回类型与赋值目标不匹配
2. 泛型类型参数未正确指定
3. 数组元素类型与声明不匹配

**解决**：检查所有相关类型的定义。

### Q: "No C compiler found" 但已安装 GCC

**可能原因**：GCC 不在系统 PATH 中。

**解决**：将 GCC 的 bin 目录添加到 PATH 环境变量。

### Q: 程序编译成功但运行时崩溃

**可能原因**：
1. 空指针解引用
2. 数组越界
3. 栈溢出
4. 除以零

**解决**：使用调试器或添加检查代码。

---

*文档版本: v0.4.0*
*最后更新: 2026-05-11*
