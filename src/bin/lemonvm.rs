use lemonc::jit::{read_module, LeVM, JitCompiler, JitState};
use std::env;
use std::fs::File;

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        eprintln!("Lemon VM (lemonvm) v1.7.0 - JIT Runtime");
        eprintln!("Usage: lemonvm <file.lmb> [options]");
        eprintln!("Options:");
        eprintln!("  --debug       Print bytecode before execution");
        eprintln!("  --jit         Enable JIT compilation (default: on)");
        eprintln!("  --no-jit      Disable JIT compilation");
        eprintln!("  --jit-threshold <n>  JIT compilation threshold (default: 100)");
        std::process::exit(1);
    }

    let input = &args[1];
    let debug = args.contains(&"--debug".to_string());
    let no_jit = args.contains(&"--no-jit".to_string());
    let jit_enabled = !no_jit;

    let mut jit_threshold = 100u64;
    if let Some(pos) = args.iter().position(|a| a == "--jit-threshold") {
        if let Some(val) = args.get(pos + 1) {
            if let Ok(n) = val.parse::<u64>() {
                jit_threshold = n;
            }
        }
    }

    if !input.ends_with(".lmb") {
        eprintln!("Error: Input file must have .lmb extension");
        std::process::exit(1);
    }

    let file = match File::open(input) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("Error opening file '{}': {}", input, e);
            std::process::exit(1);
        }
    };

    let mut reader = std::io::BufReader::new(file);
    let module = match read_module(&mut reader) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("Error reading bytecode: {}", e);
            std::process::exit(1);
        }
    };

    if debug {
        println!("Bytecode functions: {}", module.functions.len());
        println!("Bytecode classes: {}", module.classes.len());
        println!("String pool: {} strings", module.string_pool.len());
        for (i, s) in module.string_pool.iter().enumerate() {
            println!("  [{}] = {:?}", i, s);
        }

        for (i, func) in module.functions.iter().enumerate() {
            println!("\nFunction {}: {} (locals: {}, params: {:?})",
                i, func.name, func.locals, func.params);
            for (j, instr) in func.code.iter().enumerate() {
                println!("  {:3}: {:?}", j, instr);
            }
        }
        println!();
    }

    // Initialize JIT state
    let mut jit_state = JitState::new();
    jit_state.enabled = jit_enabled;
    jit_state.hot_threshold = jit_threshold;

    if jit_enabled {
        println!("JIT compilation enabled (threshold: {})", jit_threshold);

        let jit_compiler = JitCompiler::new();
        for (i, func) in module.functions.iter().enumerate() {
            if func.code.len() < 50 {
                if let Some(jit_func) = jit_compiler.compile_function(func, &module) {
                    println!("  [JIT] Pre-compiled function #{}: {}", i, func.name);
                    jit_state.register_compiled(i as u32, jit_func);
                }
            }
        }
    } else {
        println!("JIT compilation disabled");
    }

    println!("Executing in LeVM...");
    let mut vm = LeVM::new(module);
    vm.set_jit_threshold(jit_threshold);

    match vm.run() {
        Ok(result) => {
            println!("\nVM execution complete. Result: {:?}", result);
        }
        Err(e) => {
            eprintln!("VM execution error: {}", e);
            std::process::exit(1);
        }
    }
}
