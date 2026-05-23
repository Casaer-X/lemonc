mod lexer;
mod parser;
mod ast;
mod ir;
mod codegen;
mod jit;
mod diagnostics;
mod build_system;
mod linker;

use lexer::lexer::Lexer;
use parser::parser::Parser;
use codegen::c_gen::CCodeGen;
use codegen::nasm_gen::NasmCodeGen;
use codegen::native_gen::NativeCodeGen;
use jit::BytecodeCompiler;
use build_system::{AnnotationParser, ProjectBuilder, CompileTarget};
use std::env;
use std::fs;
use std::path::Path;
use std::process::Command;

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        print_usage();
        std::process::exit(1);
    }

    // 检查是否是项目构建模式
    if args.contains(&"--build".to_string()) {
        let project_dir = args.iter()
            .skip(1)
            .find(|a| !a.starts_with('-'))
            .map(|s| s.as_str())
            .unwrap_or(".");
        
        let builder = ProjectBuilder::new();
        match builder.build_project(project_dir) {
            Ok(result) => {
                if result.success {
                    println!("\nBuild successful!");
                    for file in &result.compiled_files {
                        println!("  {} -> {} ({:?})", file.source, file.output, file.target);
                    }
                } else {
                    eprintln!("\nBuild failed with errors:");
                    for error in &result.errors {
                        eprintln!("  ERROR: {}", error);
                    }
                    std::process::exit(1);
                }
            }
            Err(e) => {
                eprintln!("Build error: {}", e);
                std::process::exit(1);
            }
        }
        return;
    }

    let lex_only = args.contains(&"--lex-only".to_string());
    let dump_tokens = args.contains(&"--dump-tokens".to_string());
    let parse_only = args.contains(&"--parse-only".to_string());
    let keep_intermediate = args.contains(&"--keep-intermediate".to_string());
    let target = get_target(&args);

    let input_files: Vec<String> = args.iter()
        .skip(1)
        .filter(|a| !a.starts_with('-') && a.ends_with(".lm"))
        .cloned()
        .collect();

    if input_files.is_empty() {
        eprintln!("Error: No input file specified");
        std::process::exit(1);
    }

    let primary_input = &input_files[0];

    let mut all_declarations = Vec::new();
    let mut total_tokens = 0usize;
    let mut annotation_config = AnnotationParser::new().parse_source_annotations("");

    for (file_idx, input) in input_files.iter().enumerate() {
        let source = match fs::read_to_string(input) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Error reading file '{}': {}", input, e);
                std::process::exit(1);
            }
        };

        if file_idx == 0 {
            annotation_config = AnnotationParser::new().parse_source_annotations(&source);
        }

        println!("[1/5] Lexical analysis{}...", if input_files.len() > 1 { format!(" ({}/{})", file_idx + 1, input_files.len()) } else { String::new() });
        let lexer = Lexer::new(&source);
        let tokens: Vec<_> = lexer.collect();
        total_tokens += tokens.len();

        if dump_tokens {
            use std::io::Write;
            let mut f = std::fs::File::create("dump_tokens.txt").unwrap();
            for tok in &tokens {
                writeln!(f, "{:?}", tok).unwrap();
            }
            println!("Dumped {} tokens to dump_tokens.txt", tokens.len());
            return;
        }

        if lex_only && file_idx == input_files.len() - 1 {
            println!("  Tokenized {} tokens (total)", total_tokens);
            println!("\nLexing complete!");
            return;
        }

        println!("[2/5] Parsing{}...", if input_files.len() > 1 { format!(" ({}/{})", file_idx + 1, input_files.len()) } else { String::new() });
        let mut parser = Parser::new(tokens);
        let program = parser.parse();

        if parser.has_errors() {
            eprintln!("\nParse errors in '{}':", input);
            for error in parser.errors() {
                eprintln!("  {}", error);
            }
            std::process::exit(1);
        }

        all_declarations.extend(program.declarations);
    }

    println!("  Tokenized {} tokens (total from {} files)", total_tokens, input_files.len());

    let mut program = ast::node::Program { declarations: all_declarations };
    println!("  Parsed {} declarations (total)", program.declarations.len());

    if parse_only {
        println!("\nParsing complete!");
        return;
    }

    let effective_target = if target == "c" && annotation_config.target != CompileTarget::C {
        println!("  [Auto-detect] Using target from source annotation: {:?}", annotation_config.target);
        match annotation_config.target {
            CompileTarget::C => "c",
            CompileTarget::Nasm => "nasm",
            CompileTarget::Exe => "exe",
            CompileTarget::Native => "native",
            CompileTarget::Bytecode => "bytecode",
            CompileTarget::Library => "c",
            CompileTarget::Hybrid => "hybrid",
        }.to_string()
    } else {
        target
    };
    let output_file = get_output_file(&args, primary_input, &effective_target, annotation_config.output_name.as_deref());

    println!("\n[2.5/5] Semantic analysis...");
    let mut semantic = ast::semantic::SemanticAnalyzer::new();
    let (sem_errors, sem_warnings) = semantic.analyze(&program);
    if !sem_warnings.is_empty() {
        for warning in &sem_warnings {
            println!("  [Warning] {}", warning);
        }
    }
    if !sem_errors.is_empty() {
        eprintln!("\nSemantic errors:");
        for error in &sem_errors {
            eprintln!("  {}", error);
        }
        std::process::exit(1);
    }
    println!("  Semantic analysis passed ({} warnings)", sem_warnings.len());

    println!("\n[3/5] Generating IR...");
    let mut ir_gen = ir::gen::IRGenerator::new();
    let _module = ir_gen.generate(&program);
    println!("  IR generated successfully");

    println!("\n[4/5] Optimizing...");
    let opt_level = get_opt_level(&args, annotation_config.optimize_level);
    if opt_level > 0 {
        let mut optimizer = ast::optimizer::AstOptimizer::new();
        let stats = optimizer.optimize(&mut program);
        println!("  Constants folded: {}", stats.constants_folded);
        println!("  Constants propagated: {}", stats.constants_propagated);
        println!("  Dead code removed: {}", stats.dead_code_removed);
    } else {
        println!("  Optimization disabled (-O0)");
    }

    if effective_target == "native" {
        println!("\n[5/5] Generating native x86-64 executable...");
        let mut native_gen = NativeCodeGen::new();
        let coff_bytes = native_gen.generate(&program);

        let os = detect_os();
        let obj_path = format!("{}.obj", primary_input.trim_end_matches(".lm"));
        match fs::write(&obj_path, &coff_bytes) {
            Ok(_) => println!("  Object file written to: {}", obj_path),
            Err(e) => {
                eprintln!("Error writing object file: {}", e);
                std::process::exit(1);
            }
        }

        link_native_exe(&obj_path, &output_file, &os, keep_intermediate);

        println!("\nCompilation complete!");
        return;
    }

    if effective_target == "hybrid" {
        println!("\n[5/5] Generating hybrid AOT+JIT executable...");

        // Step 1: Compile bytecode for JIT functions
        let mut bc_compiler = BytecodeCompiler::new();
        let module = bc_compiler.compile(&program);
        println!("  Bytecode functions: {}", module.functions.len());
        println!("  Bytecode classes: {}", module.classes.len());

        // Step 2: Serialize bytecode to bytes
        let mut lmb_bytes = Vec::new();
        if let Err(e) = jit::write_module(&mut lmb_bytes, &module) {
            eprintln!("Error serializing bytecode: {}", e);
            std::process::exit(1);
        }
        println!("  Bytecode size: {} bytes", lmb_bytes.len());

        // Step 3: Generate native code
        let mut native_gen = NativeCodeGen::new();
        let coff_bytes = native_gen.generate(&program);

        // Step 4: Write object file
        let os = detect_os();
        let obj_path = format!("{}.obj", primary_input.trim_end_matches(".lm"));
        match fs::write(&obj_path, &coff_bytes) {
            Ok(_) => println!("  Object file written to: {}", obj_path),
            Err(e) => {
                eprintln!("Error writing object file: {}", e);
                std::process::exit(1);
            }
        }

        // Step 5: Link with embedded .lmb data
        link_hybrid_exe(&obj_path, &output_file, &os, &lmb_bytes, keep_intermediate);

        println!("\nCompilation complete! (hybrid AOT+JIT)");
        return;
    }

    if effective_target == "bytecode" || effective_target == "jit" {
        println!("\n[5/5] Compiling to bytecode...");
        let mut bc_compiler = BytecodeCompiler::new();
        let module = bc_compiler.compile(&program);

        println!("  Bytecode functions: {}", module.functions.len());
        println!("  Bytecode classes: {}", module.classes.len());
        println!("  String pool: {} strings", module.string_pool.len());

        // Write bytecode to file
        let lmb_path = if effective_target == "bytecode" {
            format!("{}", output_file)
        } else {
            format!("{}.lmb", primary_input.trim_end_matches(".lm"))
        };
        
        let mut file = match std::fs::File::create(&lmb_path) {
            Ok(f) => f,
            Err(e) => {
                eprintln!("Error creating bytecode file '{}': {}", lmb_path, e);
                std::process::exit(1);
            }
        };
        
        if let Err(e) = jit::write_module(&mut file, &module) {
            eprintln!("Error writing bytecode: {}", e);
            std::process::exit(1);
        }
        
        println!("  Bytecode written to: {}", lmb_path);
        println!("\nCompilation complete!");
        println!("Run with: lemonvm {}", lmb_path);
        return;
    }

    let codegen_target = if effective_target == "exe" { "c" } else { effective_target.as_str() };

    let code = match codegen_target {
        "c" => {
            println!("\n[5/5] Generating C code...");
            let mut c_gen = CCodeGen::new();
            let c_code = c_gen.generate(&program);

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

                if let Some(pos) = c_code.find("\ntypedef void") {
                    format!("{}{}{}", &c_code[..pos+1], string_defs, &c_code[pos+1..])
                } else if let Some(pos) = c_code.find("\nstruct ") {
                    format!("{}{}{}", &c_code[..pos+1], string_defs, &c_code[pos+1..])
                } else {
                    string_defs + &c_code
                }
            };

            full_code
        }
        "nasm" => {
            println!("\n[5/5] Generating NASM x86-64 assembly...");
            let mut nasm_gen = NasmCodeGen::new();
            nasm_gen.generate(&program)
        }
        _ => {
            eprintln!("Unknown target: {}", effective_target);
            std::process::exit(1);
        }
    };

    if effective_target == "exe" {
        compile_to_exe(&code, &output_file, codegen_target, keep_intermediate, primary_input);
    } else {
        match fs::write(&output_file, &code) {
            Ok(_) => {
                println!("  Output written to: {}", output_file);
            }
            Err(e) => {
                eprintln!("Error writing output: {}", e);
                std::process::exit(1);
            }
        }
    }

    println!("\nCompilation complete!");
}

fn compile_to_exe(code: &str, exe_path: &str, intermediate_target: &str, keep: bool, input_file: &str) {
    let os = detect_os();
    let compiler = find_c_compiler(&os);

    let intermediate_ext = if intermediate_target == "nasm" { "asm" } else { "c" };
    let intermediate_path = format!("{}.{}", input_file.trim_end_matches(".lm"), intermediate_ext);

    match fs::write(&intermediate_path, code) {
        Ok(_) => println!("  Intermediate written to: {}", intermediate_path),
        Err(e) => {
            eprintln!("Error writing intermediate file: {}", e);
            std::process::exit(1);
        }
    }

    if intermediate_target == "nasm" {
        compile_nasm_to_exe(&intermediate_path, exe_path, &os, keep);
    } else {
        compile_c_to_exe(&intermediate_path, exe_path, &compiler, &os, keep);
    }
}

fn compile_c_to_exe(c_path: &str, exe_path: &str, compiler: &CCompilerInfo, os: &OsInfo, keep: bool) {
    println!("\n[6/6] Compiling to executable...");
    println!("  OS: {} ({})", os.name, os.arch);
    println!("  Compiler: {} {}", compiler.name, compiler.version.as_deref().unwrap_or(""));

    let mut cmd = Command::new(&compiler.path);

    if compiler.name == "cl.exe" {
        cmd.arg(c_path);
        cmd.arg(format!("/Fe{}", exe_path));
        cmd.arg("/O2");
    } else {
        cmd.arg(c_path);
        cmd.arg("-o");
        cmd.arg(exe_path);
        cmd.arg("-O2");
        cmd.arg("-Wall");
        if os.name == "windows" {
            cmd.arg("-static");
        }
    }

    println!("  Running: {:?}", cmd);

    let output = match cmd.output() {
        Ok(o) => o,
        Err(e) => {
            eprintln!("Failed to run compiler '{}': {}", compiler.path, e);
            eprintln!("Make sure a C compiler is installed and in PATH.");
            std::process::exit(1);
        }
    };

    if !output.status.success() {
        eprintln!("\nCompiler error:");
        if !output.stdout.is_empty() {
            eprintln!("{}", String::from_utf8_lossy(&output.stdout));
        }
        if !output.stderr.is_empty() {
            eprintln!("{}", String::from_utf8_lossy(&output.stderr));
        }
        std::process::exit(1);
    }

    if !keep {
        match fs::remove_file(c_path) {
            Ok(_) => println!("  Cleaned up intermediate file: {}", c_path),
            Err(_) => {}
        }
    }

    println!("  Executable written to: {}", exe_path);
}

fn compile_nasm_to_exe(asm_path: &str, exe_path: &str, os: &OsInfo, keep: bool) {
    println!("\n[6/6] Assembling to executable...");

    let nasm_path = find_executable("nasm");
    let linker_path = find_executable("gcc");

    let obj_path = format!("{}.o", asm_path.trim_end_matches(".asm"));

    if let Some(nasm) = &nasm_path {
        let fmt = match os.name.as_str() {
            "windows" => "win64",
            "macos" => "macho64",
            _ => "elf64",
        };

        let mut cmd = Command::new(nasm);
        cmd.arg("-f").arg(fmt);
        cmd.arg("-o").arg(&obj_path);
        cmd.arg(asm_path);

        println!("  Running NASM: {:?}", cmd);

        match cmd.output() {
            Ok(output) if output.status.success() => {}
            Ok(output) => {
                eprintln!("NASM error:");
                eprintln!("{}", String::from_utf8_lossy(&output.stderr));
                std::process::exit(1);
            }
            Err(e) => {
                eprintln!("Failed to run NASM: {}", e);
                eprintln!("Make sure NASM is installed and in PATH.");
                std::process::exit(1);
            }
        }
    } else {
        eprintln!("NASM not found. Install NASM to use --target nasm --target exe.");
        std::process::exit(1);
    }

    if let Some(gcc) = &linker_path {
        let mut cmd = Command::new(gcc);
        cmd.arg(&obj_path);
        cmd.arg("-o");
        cmd.arg(exe_path);

        if os.name != "windows" {
            cmd.arg("-lm");
        }

        println!("  Running linker: {:?}", cmd);

        match cmd.output() {
            Ok(output) if output.status.success() => {}
            Ok(output) => {
                eprintln!("Linker error:");
                eprintln!("{}", String::from_utf8_lossy(&output.stderr));
                std::process::exit(1);
            }
            Err(e) => {
                eprintln!("Failed to run linker: {}", e);
                std::process::exit(1);
            }
        }
    }

    if !keep {
        let _ = fs::remove_file(asm_path);
        let _ = fs::remove_file(&obj_path);
    }

    println!("  Executable written to: {}", exe_path);
}

#[derive(Debug)]
struct OsInfo {
    name: String,
    arch: String,
    exe_ext: String,
}

fn detect_os() -> OsInfo {
    let os = env::consts::OS;
    let arch = env::consts::ARCH;

    let (name, exe_ext) = match os {
        "windows" => ("windows".to_string(), ".exe".to_string()),
        "macos" => ("macos".to_string(), String::new()),
        "linux" => ("linux".to_string(), String::new()),
        "freebsd" => ("freebsd".to_string(), String::new()),
        "openbsd" => ("openbsd".to_string(), String::new()),
        _ => (os.to_string(), String::new()),
    };

    OsInfo {
        name,
        arch: arch.to_string(),
        exe_ext,
    }
}

#[derive(Debug)]
struct CCompilerInfo {
    name: String,
    path: String,
    version: Option<String>,
}

fn find_c_compiler(os: &OsInfo) -> CCompilerInfo {
    let candidates: Vec<(String, Vec<String>)> = if os.name == "windows" {
        vec![
            ("gcc".to_string(), vec!["gcc".to_string(), "x86_64-w64-mingw32-gcc".to_string(), "gcc.exe".to_string()]),
            ("clang".to_string(), vec!["clang".to_string(), "clang.exe".to_string()]),
            ("cl.exe".to_string(), vec!["cl.exe".to_string()]),
            ("cc".to_string(), vec!["cc".to_string(), "cc.exe".to_string()]),
        ]
    } else if os.name == "macos" {
        vec![
            ("clang".to_string(), vec!["clang".to_string()]),
            ("gcc".to_string(), vec!["gcc".to_string()]),
            ("cc".to_string(), vec!["cc".to_string()]),
        ]
    } else {
        vec![
            ("gcc".to_string(), vec!["gcc".to_string(), "cc".to_string()]),
            ("clang".to_string(), vec!["clang".to_string()]),
        ]
    };

    for (name, paths) in &candidates {
        for path in paths {
            if let Some(found) = find_executable(path) {
                let version = get_compiler_version(&found, name);
                return CCompilerInfo {
                    name: name.clone(),
                    path: found,
                    version,
                };
            }
        }
    }

    eprintln!("Error: No C compiler found!");
    eprintln!("Please install one of the following:");
    eprintln!("  - GCC (https://gcc.gnu.org/)");
    eprintln!("  - Clang (https://llvm.org/)");
    if os.name == "windows" {
        eprintln!("  - MSVC (Visual Studio Build Tools)");
    }
    std::process::exit(1);
}

fn find_executable(name: &str) -> Option<String> {
    let result = if cfg!(windows) {
        Command::new("where.exe").arg(name).output()
    } else {
        Command::new("which").arg(name).output()
    };

    match result {
        Ok(output) if output.status.success() => {
            let path = String::from_utf8_lossy(&output.stdout);
            let path = path.lines().next().unwrap_or("").trim();
            if !path.is_empty() && Path::new(path).exists() {
                Some(path.to_string())
            } else {
                None
            }
        }
        _ => None,
    }
}

fn get_compiler_version(path: &str, name: &str) -> Option<String> {
    let arg = if name == "cl.exe" { "/?" } else { "--version" };
    match Command::new(path).arg(arg).output() {
        Ok(output) if output.status.success() => {
            let ver = String::from_utf8_lossy(&output.stdout);
            let first_line = ver.lines().next().unwrap_or("");
            Some(first_line.to_string())
        }
        _ => None,
    }
}

fn link_native_exe(obj_path: &str, exe_path: &str, os: &OsInfo, keep: bool) {
    println!("\n[6/6] Linking native executable...");

    if os.name == "windows" {
        if let Ok(obj_data) = fs::read(obj_path) {
            let mut pe_linker = linker::PeLinker::new();
            match pe_linker.add_object(&obj_data) {
                Ok(_) => {}
                Err(e) => {
                    eprintln!("  Built-in linker parse error: {}, trying external linker...", e);
                    link_native_exe_external(obj_path, exe_path, os, keep);
                    return;
                }
            }

            match pe_linker.build_pe() {
                Ok(pe_data) => {
                    match fs::write(exe_path, &pe_data) {
                        Ok(_) => {
                            println!("  Linked with built-in PE linker");
                            println!("  Executable written to: {}", exe_path);
                            if !keep {
                                let _ = fs::remove_file(obj_path);
                            }
                            return;
                        }
                        Err(e) => {
                            eprintln!("  Built-in linker write error: {}, trying external linker...", e);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("  Built-in linker error: {}, trying external linker...", e);
                }
            }
        }
    }

    link_native_exe_external(obj_path, exe_path, os, keep);
}

fn link_hybrid_exe(obj_path: &str, exe_path: &str, os: &OsInfo, lmb_data: &[u8], keep: bool) {
    println!("\n[6/6] Linking hybrid AOT+JIT executable...");

    if os.name == "windows" {
        if let Ok(obj_data) = fs::read(obj_path) {
            let mut pe_linker = linker::PeLinker::new();
            match pe_linker.add_object(&obj_data) {
                Ok(_) => {}
                Err(e) => {
                    eprintln!("  Built-in linker parse error: {}, trying external linker...", e);
                    link_native_exe_external(obj_path, exe_path, os, keep);
                    return;
                }
            }

            // Embed .lmb bytecode data as a PE section
            pe_linker.add_embedded_lmb(lmb_data);
            println!("  Embedded .lmb bytecode: {} bytes", lmb_data.len());

            match pe_linker.build_pe() {
                Ok(pe_data) => {
                    match fs::write(exe_path, &pe_data) {
                        Ok(_) => {
                            println!("  Linked with built-in PE linker (hybrid mode)");
                            println!("  Executable written to: {}", exe_path);
                            if !keep {
                                let _ = fs::remove_file(obj_path);
                            }
                            return;
                        }
                        Err(e) => {
                            eprintln!("  Built-in linker write error: {}, trying external linker...", e);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("  Built-in linker error: {}, trying external linker...", e);
                }
            }
        }
    }

    // Fallback: link without embedded bytecode
    link_native_exe_external(obj_path, exe_path, os, keep);
}

fn link_native_exe_external(obj_path: &str, exe_path: &str, os: &OsInfo, keep: bool) {
    let linker = find_linker(os);
    let mut cmd = Command::new(&linker);

    if linker.contains("link.exe") {
        cmd.arg("/SUBSYSTEM:CONSOLE");
        cmd.arg("/ENTRY:main");
        cmd.arg(format!("/OUT:{}", exe_path));
        cmd.arg(obj_path);
        cmd.arg("msvcrt.lib");
    } else {
        cmd.arg(obj_path);
        cmd.arg("-o");
        cmd.arg(exe_path);
        if os.name == "windows" {
            cmd.arg("-lmsvcrt");
            cmd.arg("-mconsole");
        } else {
            cmd.arg("-lm");
            cmd.arg("-lc");
        }
    }

    println!("  Linker: {:?}", cmd);

    let output = match cmd.output() {
        Ok(o) => o,
        Err(e) => {
            eprintln!("Failed to run linker '{}': {}", linker, e);
            eprintln!("Make sure a linker is installed (link.exe, ld, or gcc).");
            std::process::exit(1);
        }
    };

    if !output.status.success() {
        eprintln!("\nLinker error:");
        if !output.stdout.is_empty() {
            eprintln!("{}", String::from_utf8_lossy(&output.stdout));
        }
        if !output.stderr.is_empty() {
            eprintln!("{}", String::from_utf8_lossy(&output.stderr));
        }
        std::process::exit(1);
    }

    if !keep {
        let _ = fs::remove_file(obj_path);
    }

    println!("  Executable written to: {}", exe_path);
}

fn find_linker(os: &OsInfo) -> String {
    if os.name == "windows" {
        if let Some(link) = find_executable("link.exe") {
            return link;
        }
        if let Some(gcc) = find_executable("gcc") {
            return gcc;
        }
        if let Some(ld) = find_executable("ld") {
            return ld;
        }
    } else if os.name == "macos" {
        if let Some(ld) = find_executable("ld") {
            return ld;
        }
        if let Some(gcc) = find_executable("gcc") {
            return gcc;
        }
    } else {
        if let Some(ld) = find_executable("ld") {
            return ld;
        }
        if let Some(gcc) = find_executable("gcc") {
            return gcc;
        }
    }

    eprintln!("Error: No linker found!");
    eprintln!("Please install a linker (link.exe on Windows, ld on Linux/macOS).");
    std::process::exit(1);
}

fn get_target(args: &[String]) -> String {
    if let Some(pos) = args.iter().position(|a| a == "--target") {
        if let Some(target) = args.get(pos + 1) {
            return target.clone();
        }
    }
    "c".to_string()
}

fn get_output_file(args: &[String], input: &str, target: &str, annotation_output: Option<&str>) -> String {
    if let Some(pos) = args.iter().position(|a| a == "-o") {
        if let Some(output) = args.get(pos + 1) {
            return output.clone();
        }
    }

    if let Some(output) = annotation_output {
        return output.to_string();
    }

    let base = input.trim_end_matches(".lm");
    match target {
        "exe" => {
            let os = detect_os();
            format!("{}{}", base, os.exe_ext)
        }
        "native" => {
            let os = detect_os();
            format!("{}{}", base, os.exe_ext)
        }
        "nasm" => format!("{}.asm", base),
        "bytecode" | "jit" => format!("{}.lmb", base),
        "hybrid" => {
            let os = detect_os();
            format!("{}{}", base, os.exe_ext)
        }
        _ => format!("{}.c", base),
    }
}

fn get_opt_level(args: &[String], annotation_level: u32) -> u32 {
    for arg in args {
        if arg.starts_with("-O") {
            let level_str = &arg[2..];
            if level_str.is_empty() {
                return 2;
            }
            if let Ok(level) = level_str.parse::<u32>() {
                return level.min(3);
            }
        }
    }
    annotation_level.min(3)
}

fn print_usage() {
    eprintln!("Lemon Compiler (lemonc) v1.2.0");
    eprintln!("Usage:");
    eprintln!("  lemonc <file.lm> [options]           Compile a single file");
    eprintln!("  lemonc <f1.lm> <f2.lm> ... [options] Compile multiple files together");
    eprintln!("  lemonc --build [dir]                  Build entire project");
    eprintln!("");
    eprintln!("Options:");
    eprintln!("  -o <file>       Output file");
    eprintln!("  -O<level>       Optimization level (0-3)");
    eprintln!("  --lex-only      Only run lexer");
    eprintln!("  --dump-tokens   Dump all tokens and exit");
    eprintln!("  --parse-only    Only run lexer + parser");
    eprintln!("  --target <tgt>  Target: c, nasm, exe, native, bytecode, hybrid");
    eprintln!("  --keep-intermediate  Keep intermediate files (.c/.asm)");
    eprintln!("");
    eprintln!("Source Annotations (auto-detect target):");
    eprintln!("  // @compile target=bytecode    Auto compile to bytecode");
    eprintln!("  // @compile target=native       Auto compile to native");
    eprintln!("  // @compile target=exe          Auto compile to executable");
    eprintln!("  // @compile optimize=2          Set optimization level");
    eprintln!("  // @compile output=myapp.exe    Set output name");
    eprintln!("  // @compile dep=math,io         Declare dependencies");
    eprintln!("  // @compile entry=true          Mark as entry point");
    eprintln!("");
    eprintln!("Targets:");
    eprintln!("  c       Generate C source code (default)");
    eprintln!("  nasm    Generate NASM x86-64 assembly");
    eprintln!("  exe     Compile directly to executable");
    eprintln!("  native  Generate native x86-64 executable directly");
    eprintln!("  bytecode  Compile to bytecode file (.lmb)");
    eprintln!("");
    eprintln!("OS auto-detection:");
    eprintln!("  Windows → .exe (uses gcc/cl.exe)");
    eprintln!("  Linux   → ELF  (uses gcc/clang)");
    eprintln!("  macOS   → Mach-O (uses clang)");
}
