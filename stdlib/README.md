# Lemon Standard Library

这是 Lemon 语言的标准库草案，所有公开 API 都用 Lemon 源码编写。当前库尽量使用编译器已经支持的语法和内置运行时函数，适合作为语言自举标准库的起点。

## 模块

| 模块 | 包名 | 主要内容 |
| --- | --- | --- |
| `math.lm` | `math` | 数学常量、数值工具、整数算法、近似计算 |
| `io.lm` | `io` | 控制台输出、基础输入、格式化辅助 |
| `string.lm` | `string` | 字符串判断、查找、转换、填充、字符分类 |
| `random.lm` | `random` | 可设置种子的 LCG 随机数与 `rand` 封装 |
| `time.lm` | `time` | 时钟 ticks、秒/毫秒计时、Timer |
| `system.lm` | `system` | 进程退出、环境变量、系统命令、断言 |
| `algorithm.lm` | `algorithm` | 排序、查找、数组聚合与复制 |

## 示例

```lemon
package main;

import math.Math;
import io.Console;
import io.Format;
import random.Random;
import string.StringUtils;

public class App {
    public static void main(String[] args) {
        double root = Math.sqrtApprox(16.0);
        Format.printLabelDouble("sqrt = ", root);

        Random rng = new Random(1234);
        int value = rng.nextIntBetween(1, 7);
        Format.printLabelInt("dice = ", value);

        bool ok = StringUtils.startsWith("lemon", "lem");
        Console.printlnBool(ok);
    }
}
```

## 说明

- 每个模块都带有 `// @compile target=library` 注解，可由项目构建模式识别为库目标。
- 目前 Lemon 前端对构造器的代码生成兼容 Java 风格写法，例如 `public Random(...)`；标准库因此采用这一形式。
- `Array<int>` 相关工具依赖当前运行时内置的 `Array`/`LemonArray` 桥接。
- 部分系统能力通过编译器已有的内置 C 运行时函数暴露，例如 `printf`、`scanf`、`clock`、`rand`、`getenv`。
