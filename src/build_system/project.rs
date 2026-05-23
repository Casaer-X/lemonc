use super::annotation::*;
use crate::lexer::lexer::Lexer;
use crate::parser::parser::Parser;
use crate::ast::semantic::SemanticAnalyzer;
use crate::ast::optimizer::AstOptimizer;
use crate::codegen::c_gen::CCodeGen;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

/// 项目结构扫描器
pub struct ProjectScanner;

impl ProjectScanner {
    pub fn new() -> Self {
        Self
    }

    /// 扫描项目目录，发现所有 Lemon 源文件
    pub fn scan_project(&self, root_dir: &str) -> Result<ProjectStructure, String> {
        let root = Path::new(root_dir);
        if !root.exists() {
            return Err(format!("Project directory '{}' does not exist", root_dir));
        }

        let mut structure = ProjectStructure {
            root_dir: root_dir.to_string(),
            source_files: Vec::new(),
            modules: HashMap::new(),
            entry_points: Vec::new(),
            libraries: Vec::new(),
            tests: Vec::new(),
        };

        self.scan_directory(root, &mut structure)?;

        // 分析模块依赖关系
        self.analyze_dependencies(&mut structure)?;

        Ok(structure)
    }

    fn scan_directory(&self, dir: &Path, structure: &mut ProjectStructure) -> Result<(), String> {
        let entries = match fs::read_dir(dir) {
            Ok(e) => e,
            Err(e) => return Err(format!("Failed to read directory '{}': {}", dir.display(), e)),
        };

        for entry in entries {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };

            let path = entry.path();
            let file_name = path.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("");

            // 跳过隐藏目录和常见非源码目录
            if file_name.starts_with('.') || file_name == "target" || file_name == "build" || file_name == "output" {
                continue;
            }

            if path.is_dir() {
                self.scan_directory(&path, structure)?;
            } else if path.extension().and_then(|e| e.to_str()) == Some("lm") {
                self.process_source_file(&path, structure)?;
            }
        }

        Ok(())
    }

    fn process_source_file(&self, path: &Path, structure: &mut ProjectStructure) -> Result<(), String> {
        let content = match fs::read_to_string(path) {
            Ok(c) => c,
            Err(e) => return Err(format!("Failed to read '{}': {}", path.display(), e)),
        };

        let parser = AnnotationParser::new();
        let config = parser.parse_source_annotations(&content);

        let path_str = path.to_string_lossy().to_string();
        let relative_path = self.get_relative_path(&structure.root_dir, path);
        let module_name = self.infer_module_name(path);

        // 分类文件
        if config.is_entry_point {
            structure.entry_points.push(path_str.clone());
        }

        if config.target == CompileTarget::Library {
            structure.libraries.push(path_str.clone());
        }

        if relative_path.contains("test") || relative_path.contains("Test") || relative_path.contains("spec") {
            structure.tests.push(path_str.clone());
        }

        structure.source_files.push(path_str.clone());
        structure.modules.insert(path_str.clone(), SourceFile {
            path: path_str,
            relative_path,
            module_name,
            config,
            content,
        });

        Ok(())
    }

    fn analyze_dependencies(&self, structure: &mut ProjectStructure) -> Result<(), String> {
        let all_deps: Vec<(String, Vec<String>)> = structure.modules
            .iter()
            .map(|(path, file_info)| {
                let mut deps = Vec::new();

                // 从 import 语句解析依赖
                for line in file_info.content.lines() {
                    let trimmed = line.trim();
                    if trimmed.starts_with("import ") {
                        let import_path = trimmed
                            .trim_start_matches("import ")
                            .trim_end_matches(';')
                            .trim();
                        
                        // 查找对应的源文件
                        let dep_file = self.find_import_source(&structure.modules, import_path);
                        if let Some(dep) = dep_file {
                            if dep != *path {
                                deps.push(dep);
                            }
                        }
                    }
                }

                // 合并注解中声明的依赖
                for dep in &file_info.config.dependencies {
                    let dep_file = self.find_import_source(&structure.modules, dep);
                    if let Some(dep_path) = dep_file {
                        if !deps.contains(&dep_path) && dep_path != *path {
                            deps.push(dep_path);
                        }
                    }
                }

                (path.clone(), deps)
            })
            .collect();

        for (path, deps) in all_deps {
            if let Some(file) = structure.modules.get_mut(&path) {
                file.config.dependencies = deps;
            }
        }

        Ok(())
    }

    fn find_import_source(&self, modules: &HashMap<String, SourceFile>, import_path: &str) -> Option<String> {
        // 将 import 路径转换为可能的文件名
        let possible_names = [
            format!("{}.lm", import_path.replace('.', "/")),
            format!("{}.lm", import_path.replace('.', "\\")),
            format!("{}.lm", import_path),
        ];

        for (path, file) in modules {
            for name in &possible_names {
                if file.relative_path.ends_with(name) || path.ends_with(name) {
                    return Some(path.clone());
                }
            }
        }

        None
    }

    fn infer_module_name(&self, path: &Path) -> String {
        path.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string()
    }

    fn get_relative_path(&self, root: &str, path: &Path) -> String {
        let root_path = Path::new(root);
        path.strip_prefix(root_path)
            .unwrap_or(path)
            .to_string_lossy()
            .to_string()
    }
}

impl Default for ProjectScanner {
    fn default() -> Self {
        Self::new()
    }
}

/// 项目结构信息
#[derive(Debug, Clone)]
pub struct ProjectStructure {
    pub root_dir: String,
    pub source_files: Vec<String>,
    pub modules: HashMap<String, SourceFile>,
    pub entry_points: Vec<String>,
    pub libraries: Vec<String>,
    pub tests: Vec<String>,
}

/// 源文件信息
#[derive(Debug, Clone)]
pub struct SourceFile {
    pub path: String,
    pub relative_path: String,
    pub module_name: String,
    pub config: ModuleConfig,
    pub content: String,
}

/// 项目构建器
pub struct ProjectBuilder {
    scanner: ProjectScanner,
}

impl ProjectBuilder {
    pub fn new() -> Self {
        Self {
            scanner: ProjectScanner::new(),
        }
    }

    /// 构建整个项目 — 合并所有源文件为一个 Program 后统一编译
    pub fn build_project(&self, root_dir: &str) -> Result<BuildResult, String> {
        println!("Scanning project at '{}'...", root_dir);
        let structure = self.scanner.scan_project(root_dir)?;

        println!("Found {} source files", structure.source_files.len());
        println!("  Entry points: {}", structure.entry_points.len());
        println!("  Libraries: {}", structure.libraries.len());
        println!("  Tests: {}", structure.tests.len());

        let mut result = BuildResult {
            success: true,
            compiled_files: Vec::new(),
            errors: Vec::new(),
            warnings: Vec::new(),
        };

        // 分离测试文件和非测试文件
        let test_files: HashSet<String> = structure.tests.iter().cloned().collect();

        // 收集非测试源文件，按依赖顺序排列
        let compile_order = self.resolve_compile_order(&structure)?;

        // === 阶段1：合并所有非测试文件为一个 Program，统一编译 ===
        let main_files: Vec<String> = compile_order.iter()
            .filter(|p| !test_files.contains(*p))
            .cloned()
            .collect();

        if !main_files.is_empty() {
            println!("\n=== Merging {} source files into one compilation unit ===", main_files.len());
            match self.compile_merged(&main_files, &structure) {
                Ok(output) => {
                    for path in &main_files {
                        if let Some(file) = structure.modules.get(path) {
                            result.compiled_files.push(CompiledFile {
                                source: file.path.clone(),
                                output: output.clone(),
                                target: CompileTarget::C,
                            });
                        }
                    }
                }
                Err(e) => {
                    result.errors.push(format!("Merged compilation: {}", e));
                    result.success = false;
                }
            }
        }

        // === 阶段2：单独编译测试文件（它们可能有特殊的语义错误预期） ===
        for test_path in &structure.tests {
            if let Some(file) = structure.modules.get(test_path) {
                println!("\nCompiling test: {}", file.relative_path);
                match self.compile_single_file(file) {
                    Ok(output) => {
                        result.compiled_files.push(CompiledFile {
                            source: file.path.clone(),
                            output,
                            target: file.config.target.clone(),
                        });
                    }
                    Err(e) => {
                        // 测试文件编译失败不标记整体失败（测试可能故意包含错误）
                        result.warnings.push(format!("Test {} compilation: {}", file.relative_path, e));
                    }
                }
            }
        }

        Ok(result)
    }

    /// 合并多个文件为一个 Program 并编译
    fn compile_merged(&self, file_paths: &[String], structure: &ProjectStructure) -> Result<String, String> {
        let mut all_declarations = Vec::new();
        let mut total_tokens = 0usize;
        let mut annotation_config = ModuleConfig::default();

        for (file_idx, file_path) in file_paths.iter().enumerate() {
            let file = structure.modules.get(file_path)
                .ok_or_else(|| format!("File not found: {}", file_path))?;

            println!("[1/5] Lexical analysis ({}/{})...", file_idx + 1, file_paths.len());

            let lexer = Lexer::new(&file.content);
            let tokens: Vec<_> = lexer.collect();
            total_tokens += tokens.len();

            println!("[2/5] Parsing ({}/{})...", file_idx + 1, file_paths.len());
            let mut parser = Parser::new(tokens);
            let program = parser.parse();

            if parser.has_errors() {
                let errors: Vec<String> = parser.errors().iter().map(|e| e.to_string()).collect();
                return Err(format!("Parse errors in '{}':\n{}", file.relative_path, errors.join("\n")));
            }

            if file_idx == 0 {
                annotation_config = file.config.clone();
            }

            all_declarations.extend(program.declarations);
        }

        println!("  Tokenized {} tokens (total from {} files)", total_tokens, file_paths.len());

        let mut program = crate::ast::node::Program { declarations: all_declarations };
        println!("  Parsed {} declarations (total)", program.declarations.len());

        // 语义分析
        println!("\n[2.5/5] Semantic analysis...");
        let mut semantic = SemanticAnalyzer::new();
        let (sem_errors, sem_warnings) = semantic.analyze(&program);
        for warning in &sem_warnings {
            println!("  [Warning] {}", warning);
        }
        if !sem_errors.is_empty() {
            let errors: Vec<String> = sem_errors.iter().map(|e| e.to_string()).collect();
            return Err(format!("Semantic errors:\n{}", errors.join("\n")));
        }
        println!("  Semantic analysis passed ({} warnings)", sem_warnings.len());

        // 优化
        println!("\n[3/5] Optimizing...");
        let opt_level = annotation_config.optimize_level;
        if opt_level > 0 {
            let mut optimizer = AstOptimizer::new();
            let stats = optimizer.optimize(&mut program);
            println!("  Constants folded: {}", stats.constants_folded);
            println!("  Constants propagated: {}", stats.constants_propagated);
            println!("  Dead code removed: {}", stats.dead_code_removed);
        }

        // 代码生成
        println!("\n[4/5] Generating C code...");
        let mut c_gen = CCodeGen::new();
        let c_code = c_gen.generate(&program);

        // 收集字符串字面量并插入到生成的 C 代码中
        let strings = c_gen.string_literals().to_vec();
        let full_code = if strings.is_empty() {
            c_code
        } else {
            let mut string_defs = String::new();
            for (i, s) in strings.iter().enumerate() {
                let escaped = s.replace('\\', "\\\\")
                    .replace('"', "\\\"")
                    .replace('\n', "\\n")
                    .replace('\r', "\\r")
                    .replace('\t', "\\t")
                    .replace('\0', "\\0");
                string_defs.push_str(&format!("static const char _str_{}[] = \"{}\";\n", i, escaped));
            }
            string_defs.push_str("\n");

            // 在第一个 typedef 或 struct 定义之前插入字符串字面量
            if let Some(pos) = c_code.find("\ntypedef void") {
                format!("{}{}{}", &c_code[..pos+1], string_defs, &c_code[pos+1..])
            } else if let Some(pos) = c_code.find("\nstruct ") {
                format!("{}{}{}", &c_code[..pos+1], string_defs, &c_code[pos+1..])
            } else {
                string_defs + &c_code
            }
        };

        // 确定输出文件名
        let output_name = annotation_config.output_name.clone()
            .unwrap_or_else(|| {
                // 使用项目目录名作为输出文件名
                let dir_name = Path::new(&structure.root_dir)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("output");
                format!("{}.c", dir_name)
            });

        // 写入输出文件
        let output_path = if Path::new(&output_name).is_absolute() {
            output_name.clone()
        } else {
            format!("{}/{}", structure.root_dir, output_name)
        };

        fs::write(&output_path, &full_code)
            .map_err(|e| format!("Error writing '{}': {}", output_path, e))?;

        println!("  Output written to: {}", output_path);
        println!("\nCompilation complete!");

        Ok(output_path)
    }

    /// 编译单个文件（用于测试文件）
    fn compile_single_file(&self, file: &SourceFile) -> Result<String, String> {
        let output_name = file.config.output_name.clone()
            .unwrap_or_else(|| file.config.target.default_output_name(&file.module_name));

        let args = self.build_compiler_args(file, &output_name);

        // 获取 lemonc 可执行文件路径
        let lemonc_path = std::env::current_exe()
            .map_err(|e| format!("Failed to get current executable path: {}", e))?;

        let mut cmd = std::process::Command::new(&lemonc_path);
        for arg in &args {
            cmd.arg(arg);
        }

        let output = cmd.output()
            .map_err(|e| format!("Failed to execute lemonc: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("Compilation failed: {}", stderr));
        }

        Ok(output_name)
    }

    fn build_compiler_args(&self, file: &SourceFile, output: &str) -> Vec<String> {
        let mut args = vec![
            file.path.clone(),
            format!("-O{}", file.config.optimize_level),
            "-o".to_string(),
            output.to_string(),
        ];

        let target_str = match file.config.target {
            CompileTarget::C => "c",
            CompileTarget::Nasm => "nasm",
            CompileTarget::Exe => "exe",
            CompileTarget::Native => "native",
            CompileTarget::Bytecode => "bytecode",
            CompileTarget::Library => "c",
            CompileTarget::Hybrid => "hybrid",
        };

        args.push("--target".to_string());
        args.push(target_str.to_string());

        args
    }

    /// 解析编译顺序（拓扑排序）
    fn resolve_compile_order(&self, structure: &ProjectStructure) -> Result<Vec<String>, String> {
        let mut order = Vec::new();
        let mut visited = HashMap::new();

        // 先编译库
        for lib in &structure.libraries {
            self.visit_module(structure, lib, &mut visited, &mut order)?;
        }

        // 再编译入口点
        for entry in &structure.entry_points {
            self.visit_module(structure, entry, &mut visited, &mut order)?;
        }

        // 最后编译其他文件
        for file in &structure.source_files {
            if !order.contains(file) {
                self.visit_module(structure, file, &mut visited, &mut order)?;
            }
        }

        Ok(order)
    }

    fn visit_module(
        &self,
        structure: &ProjectStructure,
        path: &str,
        visited: &mut HashMap<String, bool>,
        order: &mut Vec<String>,
    ) -> Result<(), String> {
        if let Some(&in_progress) = visited.get(path) {
            if in_progress {
                return Err(format!("Circular dependency detected involving '{}'", path));
            }
            return Ok(());
        }

        visited.insert(path.to_string(), true);

        // 先编译依赖
        if let Some(file) = structure.modules.get(path) {
            for dep in &file.config.dependencies {
                self.visit_module(structure, dep, visited, order)?;
            }
        }

        visited.insert(path.to_string(), false);
        order.push(path.to_string());

        Ok(())
    }
}

impl Default for ProjectBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// 构建结果
#[derive(Debug, Clone)]
pub struct BuildResult {
    pub success: bool,
    pub compiled_files: Vec<CompiledFile>,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

/// 编译后的文件
#[derive(Debug, Clone)]
pub struct CompiledFile {
    pub source: String,
    pub output: String,
    pub target: CompileTarget,
}
