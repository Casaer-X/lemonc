# Lemon Standard Library

Lemon 语言标准库，提供常用的工具类和函数。

## 模块列表

| 模块 | 包名 | 说明 |
|------|------|------|
| [Math](math.lm) | `math` | 数学函数和常量 |
| [IO](io.lm) | `io` | 输入输出操作 |
| [String](string.lm) | `string` | 字符串工具 |
| [Random](random.lm) | `random` | 随机数生成 |
| [Time](time.lm) | `time` | 时间和日期工具 |
| [System](system.lm) | `system` | 系统级工具 |
| [Algorithm](algorithm.lm) | `algorithm` | 常用算法 |

## 使用方法

```lemon
package main;

import math.Math;
import io.Console;
import random.RandomUtils;

public class MyApp {
    public static void main(String[] args) {
        // 数学运算
        double r = Math.sqrtApprox(16.0);
        double angle = Math.toRadians(90.0);

        // 控制台输出
        Console.println("Hello, Lemon!");
        Console.printInt(42);

        // 随机数
        int dice = RandomUtils.randomInt(1, 7);
        Console.printInt(dice);
    }
}
```

## 编译标准库

```bash
# 编译整个标准库为库文件
cd stdlib
lemonc --build

# 编译单个模块
lemonc math.lm --target library
```

## 依赖关系

- `math` - 无依赖
- `io` - 无依赖
- `string` - 无依赖
- `random` - 无依赖
- `time` - 无依赖
- `system` - 无依赖
- `algorithm` - 无依赖

## 注意事项

1. 所有模块使用 `// @compile target=library` 注解标记为库
2. 包名使用简单标识符（如 `math`），通过目录结构组织
3. 部分功能为纯 Lemon 实现，不依赖 C 标准库
