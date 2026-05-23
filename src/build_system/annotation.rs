use crate::ast::node::*;
use std::collections::HashMap;

/// 编译目标类型
#[derive(Debug, Clone, PartialEq)]
pub enum CompileTarget {
    C,
    Nasm,
    Exe,
    Native,
    Bytecode,
    Library,
    Hybrid,
}

impl CompileTarget {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "c" => Some(CompileTarget::C),
            "nasm" => Some(CompileTarget::Nasm),
            "exe" | "executable" => Some(CompileTarget::Exe),
            "native" => Some(CompileTarget::Native),
            "bytecode" | "lmb" => Some(CompileTarget::Bytecode),
            "lib" | "library" => Some(CompileTarget::Library),
            "hybrid" => Some(CompileTarget::Hybrid),
            _ => None,
        }
    }

    pub fn file_extension(&self) -> &str {
        match self {
            CompileTarget::C => ".c",
            CompileTarget::Nasm => ".asm",
            CompileTarget::Exe => ".exe",
            CompileTarget::Native => ".exe",
            CompileTarget::Bytecode => ".lmb",
            CompileTarget::Library => ".a",
            CompileTarget::Hybrid => ".exe",
        }
    }

    pub fn default_output_name(&self, input_name: &str) -> String {
        let base = input_name.trim_end_matches(".lm");
        format!("{}{}", base, self.file_extension())
    }
}

/// 模块编译配置（从注解解析）
#[derive(Debug, Clone)]
pub struct ModuleConfig {
    pub target: CompileTarget,
    pub output_name: Option<String>,
    pub optimize_level: u32,
    pub dependencies: Vec<String>,
    pub features: Vec<String>,
    pub is_entry_point: bool,
}

impl Default for ModuleConfig {
    fn default() -> Self {
        Self {
            target: CompileTarget::C,
            output_name: None,
            optimize_level: 1,
            dependencies: Vec::new(),
            features: Vec::new(),
            is_entry_point: false,
        }
    }
}

/// 注解解析器
pub struct AnnotationParser;

impl AnnotationParser {
    pub fn new() -> Self {
        Self
    }

    /// 从 AST 解析模块编译配置
    pub fn parse_module_config(&self, program: &Program) -> ModuleConfig {
        let mut config = ModuleConfig::default();

        for decl in &program.declarations {
            if let Declaration::Class(class) = decl {
                for member in &class.members {
                    if let ClassMember::Method(method) = member {
                        if method.name == "main" && method.modifiers.iter().any(|m| matches!(m, MethodModifier::Static)) {
                            config.is_entry_point = true;
                            return config;
                        }
                    }
                }
            }
        }

        config
    }

    /// 从源代码文本解析编译注解
    /// 支持的注解格式：
    /// // @compile target=bytecode
    /// // @compile output=myapp.exe
    /// // @compile optimize=2
    /// // @compile dep=math,dep=io
    /// // @compile feature=gc,feature=reflection
    pub fn parse_source_annotations(&self, source: &str) -> ModuleConfig {
        let mut config = ModuleConfig::default();

        for line in source.lines() {
            let trimmed = line.trim();
            
            // 检查编译注解注释
            if let Some(annotation) = Self::extract_annotation(trimmed) {
                Self::apply_annotation(&mut config, &annotation);
            }
        }

        config
    }

    fn extract_annotation(line: &str) -> Option<(String, String)> {
        // 支持格式：
        // // @compile key=value
        // /* @compile key=value */
        // # @compile key=value
        
        let annotation_prefixes = [
            "// @compile ",
            "/* @compile ",
            "# @compile ",
        ];

        for prefix in &annotation_prefixes {
            if let Some(content) = line.strip_prefix(prefix) {
                let content = content.trim_end_matches("*/").trim();
                if let Some(eq_pos) = content.find('=') {
                    let key = content[..eq_pos].trim().to_string();
                    let value = content[eq_pos + 1..].trim().to_string();
                    return Some((key, value));
                }
            }
        }

        None
    }

    fn apply_annotation(config: &mut ModuleConfig, annotation: &(String, String)) {
        let (key, value) = annotation;
        
        match key.as_str() {
            "target" | "t" => {
                if let Some(target) = CompileTarget::from_str(value) {
                    config.target = target;
                }
            }
            "output" | "o" => {
                config.output_name = Some(value.clone());
            }
            "optimize" | "opt" | "O" => {
                if let Ok(level) = value.parse::<u32>() {
                    config.optimize_level = level.min(3);
                }
            }
            "dep" | "dependency" | "deps" => {
                for dep in value.split(',') {
                    let dep = dep.trim();
                    if !dep.is_empty() && !config.dependencies.contains(&dep.to_string()) {
                        config.dependencies.push(dep.to_string());
                    }
                }
            }
            "feature" | "features" => {
                for feature in value.split(',') {
                    let feature = feature.trim();
                    if !feature.is_empty() && !config.features.contains(&feature.to_string()) {
                        config.features.push(feature.to_string());
                    }
                }
            }
            "entry" | "main" => {
                config.is_entry_point = value == "true" || value == "1" || value == "yes";
            }
            _ => {}
        }
    }
}

impl Default for AnnotationParser {
    fn default() -> Self {
        Self::new()
    }
}

/// 项目构建配置
#[derive(Debug, Clone)]
pub struct BuildConfig {
    pub modules: HashMap<String, ModuleConfig>,
    pub default_target: CompileTarget,
    pub project_name: String,
    pub output_dir: String,
    pub build_dir: String,
}

impl Default for BuildConfig {
    fn default() -> Self {
        Self {
            modules: HashMap::new(),
            default_target: CompileTarget::C,
            project_name: "lemon_project".to_string(),
            output_dir: "output".to_string(),
            build_dir: "build".to_string(),
        }
    }
}
