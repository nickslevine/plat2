use clap::{Parser, Subcommand};
use colored::*;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{self, Command};
use anyhow::{Context, Result};
use plat_modules::{ModuleResolver, ModuleError};

#[derive(Parser)]
#[command(name = "plat")]
#[command(about = "The Plat programming language compiler", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Build a Plat source file into an executable
    Build {
        /// The Plat source file to build (optional - builds all .plat files in current directory if not specified)
        file: Option<PathBuf>,
    },
    /// Run a Plat source file
    Run {
        /// The Plat source file to run (optional - looks for main.plat if not specified)
        file: Option<PathBuf>,
    },
    /// Format a Plat source file
    Fmt {
        /// The Plat source file to format
        file: PathBuf,
    },
    /// Run tests in a Plat source file
    Test {
        /// The Plat source file to test (optional - tests all .plat files in current directory if not specified)
        file: Option<PathBuf>,
    },
    /// Run benchmarks in a Plat source file
    Bench {
        /// The Plat source file to benchmark (optional - benchmarks all .plat files in current directory if not specified)
        file: Option<PathBuf>,
    },
}

fn main() {
    if let Err(e) = run() {
        eprintln!("{}: {}", "error".red().bold(), e);
        process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Build { file } => build_command(file),
        Commands::Run { file } => run_command(file),
        Commands::Fmt { file } => fmt_command(file),
        Commands::Test { file } => test_command(file),
        Commands::Bench { file } => bench_command(file),
    }
}

fn build_command(file: Option<PathBuf>) -> Result<()> {
    match file {
        Some(f) => build_single_file(f),
        None => build_project(),
    }
}

fn build_single_file(file: PathBuf) -> Result<()> {
    validate_plat_file(&file)?;

    let source = fs::read_to_string(&file)
        .with_context(|| format!("Failed to read file: {}", file.display()))?;

    let output_path = get_output_path(&file);

    println!("{} {}", "Building".green().bold(), file.display());

    // Parse the source code
    println!("  {} Lexing...", "→".cyan());
    println!("  {} Parsing...", "→".cyan());

    let parser = plat_parser::Parser::new(&source)
        .with_context(|| "Failed to create parser")?;
    let mut program = parser.parse()
        .with_context(|| "Failed to parse program")?;

    // Type check the program
    println!("  {} Type checking...", "→".cyan());
    let type_checker = plat_hir::TypeChecker::new();
    if let Err(e) = type_checker.check_program(&mut program) {
        println!("Type checking error: {:?}", e);
        anyhow::bail!("Type checking failed: {:?}", e);
    }

    println!("  {} Generating code...", "→".cyan());

    // Generate native code using Cranelift
    let codegen = plat_codegen::CodeGenerator::new()
        .with_context(|| "Failed to initialize code generator")?;
    match codegen.generate_code(&program) {
        Ok(object_bytes) => {
            println!("  {} Linking...", "→".cyan());

            // Create output directory if it doesn't exist
            if let Some(parent) = output_path.parent() {
                std::fs::create_dir_all(parent)
                    .with_context(|| format!("Failed to create output directory: {}", parent.display()))?;
            }

            // Write object file
            let object_file = output_path.with_extension("o");
            std::fs::write(&object_file, &object_bytes)
                .with_context(|| format!("Failed to write object file: {}", object_file.display()))?;

            // Build the runtime library first
            let build_result = Command::new("cargo")
                .args(&["build", "--lib", "--package", "plat-runtime"])
                .current_dir(get_project_root()?)
                .output()
                .with_context(|| "Failed to build runtime library")?;

            if !build_result.status.success() {
                anyhow::bail!("Runtime library build failed: {}",
                    String::from_utf8_lossy(&build_result.stderr));
            }

            // Find the built runtime library
            let target_dir = get_project_root()?.join("target").join("debug");
            let runtime_lib = if cfg!(target_os = "macos") {
                target_dir.join("libplat_runtime.dylib")
            } else if cfg!(target_os = "windows") {
                target_dir.join("plat_runtime.dll")
            } else {
                target_dir.join("libplat_runtime.so")
            };

            // Link the object file with the runtime library into an executable
            let link_result = Command::new("cc")
                .arg("-o")
                .arg(&output_path)
                .arg(&object_file)
                .arg(&runtime_lib)
                .output()
                .with_context(|| "Failed to run linker")?;

            if !link_result.status.success() {
                let stderr = String::from_utf8_lossy(&link_result.stderr);
                anyhow::bail!("Linking failed:\n{}", stderr);
            }

            // Clean up object file
            std::fs::remove_file(&object_file).ok();

            println!("{} Generated executable: {}", "✓".green().bold(), output_path.display());
        }
        Err(e) => {
            anyhow::bail!("Code generation failed: {}", e);
        }
    }

    Ok(())
}

fn build_project() -> Result<()> {
    println!("{} Building project (all .plat files)", "Building".green().bold());

    let current_dir = std::env::current_dir()
        .with_context(|| "Failed to get current directory")?;

    // Discover all .plat files
    println!("  {} Discovering modules...", "→".cyan());
    let files = discover_plat_files(&current_dir)?;

    if files.is_empty() {
        anyhow::bail!("No .plat files found in current directory");
    }

    println!("  {} Found {} module(s)", "→".cyan(), files.len());

    // Resolve module dependencies
    println!("  {} Resolving dependencies...", "→".cyan());
    let ordered_files = resolve_modules(&files, &current_dir)?;

    println!("  {} Compilation order: {}", "→".cyan(),
        ordered_files.iter()
            .map(|f| f.file_name().unwrap().to_string_lossy())
            .collect::<Vec<_>>()
            .join(" → "));

    // Build all modules together with cross-module symbol resolution
    build_multi_module(&ordered_files)?;

    println!("\n{} Project built successfully", "✓".green().bold());

    Ok(())
}

/// Build multiple modules together with cross-module symbol resolution
fn build_multi_module(ordered_files: &[PathBuf]) -> Result<()> {
    // Phase 1: Parse all modules
    println!("\n  {} Parsing all modules...", "→".cyan());
    let mut modules = Vec::new();
    for file in ordered_files {
        let source = fs::read_to_string(file)
            .with_context(|| format!("Failed to read file: {}", file.display()))?;

        let parser = plat_parser::Parser::new(&source)
            .with_context(|| "Failed to create parser")?;
        let program = parser.parse()
            .with_context(|| "Failed to parse program")?;

        modules.push((file.clone(), program));
    }

    // Phase 2: Build global symbol table from all modules
    println!("  {} Building global symbol table...", "→".cyan());
    let mut global_symbols = plat_hir::ModuleSymbolTable::new(String::new());

    for (file_path, program) in &modules {
        let module_path = program.module_decl
            .as_ref()
            .map(|m| m.path.join("::"))
            .unwrap_or_default();

        // Register all top-level symbols from this module
        let mut temp_checker = plat_hir::TypeChecker::new();
        temp_checker.collect_symbols_from_program(program, &module_path, &mut global_symbols)?;
    }

    // Phase 3: Type check all modules with access to global symbols
    println!("  {} Type checking all modules...", "→".cyan());
    for (file_path, program) in &mut modules {
        let module_path = program.module_decl
            .as_ref()
            .map(|m| m.path.join("::"))
            .unwrap_or_default();

        // Clone the global symbols and set the current module
        let mut module_symbols = global_symbols.clone();
        module_symbols.current_module = module_path.clone();

        // Add imports for this module
        for use_decl in &program.use_decls {
            let import_path = use_decl.path.join("::");
            module_symbols.add_import(import_path);
        }

        let type_checker = plat_hir::TypeChecker::with_symbols(module_symbols);

        if let Err(e) = type_checker.check_program(program) {
            println!("Type checking error in {}: {:?}", file_path.display(), e);
            anyhow::bail!("Type checking failed in {}: {:?}", file_path.display(), e);
        }
    }

    // Phase 4: Generate object files for all modules
    println!("  {} Generating code for all modules...", "→".cyan());
    let mut object_files = Vec::new();

    for (file_path, program) in &modules {
        let codegen = plat_codegen::CodeGenerator::new()
            .with_context(|| "Failed to initialize code generator")?;

        let object_bytes = codegen.generate_code(program)
            .with_context(|| format!("Code generation failed for {}", file_path.display()))?;

        let object_file = file_path.with_extension("o");
        std::fs::write(&object_file, &object_bytes)
            .with_context(|| format!("Failed to write object file: {}", object_file.display()))?;

        object_files.push(object_file);
    }

    // Phase 5: Link all object files together
    println!("  {} Linking {} object file(s)...", "→".cyan(), object_files.len());

    // Build the runtime library first
    let build_result = Command::new("cargo")
        .args(&["build", "--lib", "--package", "plat-runtime"])
        .current_dir(get_project_root()?)
        .output()
        .with_context(|| "Failed to build runtime library")?;

    if !build_result.status.success() {
        anyhow::bail!("Runtime library build failed: {}",
            String::from_utf8_lossy(&build_result.stderr));
    }

    // Find the built runtime library
    let target_dir = get_project_root()?.join("target").join("debug");
    let runtime_lib = if cfg!(target_os = "macos") {
        target_dir.join("libplat_runtime.dylib")
    } else if cfg!(target_os = "windows") {
        target_dir.join("plat_runtime.dll")
    } else {
        target_dir.join("libplat_runtime.so")
    };

    // Find the main module (the one with main() function)
    let main_file = ordered_files.iter()
        .find(|f| {
            if let Ok(src) = fs::read_to_string(f) {
                src.contains("fn main(")
            } else {
                false
            }
        })
        .ok_or_else(|| anyhow::anyhow!("No main() function found in any module"))?;

    let output_path = get_output_path(main_file);

    // Create output directory if it doesn't exist
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create output directory: {}", parent.display()))?;
    }

    // Link all object files together
    let mut link_command = Command::new("cc");
    link_command.arg("-o").arg(&output_path);

    for obj_file in &object_files {
        link_command.arg(obj_file);
    }

    link_command.arg(&runtime_lib);

    let link_result = link_command
        .output()
        .with_context(|| "Failed to run linker")?;

    if !link_result.status.success() {
        let stderr = String::from_utf8_lossy(&link_result.stderr);
        anyhow::bail!("Linking failed:\n{}", stderr);
    }

    // Clean up object files
    for obj_file in &object_files {
        std::fs::remove_file(obj_file).ok();
    }

    println!("{} Generated executable: {}", "✓".green().bold(), output_path.display());

    Ok(())
}

fn run_command(file: Option<PathBuf>) -> Result<()> {
    let file_to_run = match file {
        Some(f) => f,
        None => {
            // Look for main.plat in current directory
            let main_file = PathBuf::from("main.plat");
            if !main_file.exists() {
                anyhow::bail!("No file specified and main.plat not found in current directory");
            }
            main_file
        }
    };

    validate_plat_file(&file_to_run)?;

    println!("{} {}", "Running".green().bold(), file_to_run.display());

    // First build the file
    build_command(Some(file_to_run.clone()))?;

    // Then execute the output
    let output_path = get_output_path(&file_to_run);

    println!("{} Executing {}", "→".cyan(), output_path.display());

    let run_result = Command::new(&output_path)
        .output()
        .with_context(|| format!("Failed to execute binary: {}", output_path.display()))?;

    // Print stdout
    if !run_result.stdout.is_empty() {
        print!("{}", String::from_utf8_lossy(&run_result.stdout));
    }

    // Print stderr
    if !run_result.stderr.is_empty() {
        eprint!("{}", String::from_utf8_lossy(&run_result.stderr));
    }

    // Check exit status and propagate it
    if !run_result.status.success() {
        if let Some(code) = run_result.status.code() {
            println!("{} Process exited with code: {}", "ℹ".yellow().bold(), code);
            process::exit(code);
        }
    }

    Ok(())
}

fn fmt_command(file: PathBuf) -> Result<()> {
    validate_plat_file(&file)?;

    let source = fs::read_to_string(&file)
        .with_context(|| format!("Failed to read file: {}", file.display()))?;

    println!("{} {}", "Formatting".green().bold(), file.display());

    let formatted = plat_fmt::Formatter::format(&source)
        .with_context(|| "Failed to format file")?;

    fs::write(&file, formatted)
        .with_context(|| format!("Failed to write formatted file: {}", file.display()))?;

    println!("{} Formatted successfully", "✓".green().bold());

    Ok(())
}

fn test_command(file: Option<PathBuf>) -> Result<()> {
    match file {
        Some(f) => test_single_file(f),
        None => test_project(),
    }
}

fn test_single_file(file: PathBuf) -> Result<()> {
    validate_plat_file(&file)?;

    let source = fs::read_to_string(&file)
        .with_context(|| format!("Failed to read file: {}", file.display()))?;

    println!("{}", "Running tests...".green().bold());

    // Parse the source code
    let parser = plat_parser::Parser::new(&source)
        .with_context(|| "Failed to create parser")?;
    let mut program = parser.parse()
        .with_context(|| "Failed to parse program")?;

    // Discover all test functions
    let test_functions = discover_tests(&program);

    if test_functions.is_empty() {
        println!("{} No tests found", "✓".yellow().bold());
        return Ok(());
    }

    // Generate test runner main function
    let test_main = generate_test_main(&test_functions);

    // Parse the test main function
    let test_main_parser = plat_parser::Parser::new(&test_main)
        .with_context(|| "Failed to create parser for test main")?;
    let test_main_program = test_main_parser.parse()
        .with_context(|| "Failed to parse test main")?;

    // Replace or add the main function while keeping test blocks
    if let Some(main_idx) = program.functions.iter().position(|f| f.name == "main") {
        program.functions[main_idx] = test_main_program.functions[0].clone();
    } else {
        program.functions.push(test_main_program.functions[0].clone());
    }

    // Compile and run the test program
    let output_path = get_output_path(&file);
    compile_test_program(&mut program, &output_path)?;

    // Execute the tests
    let test_result = Command::new(&output_path)
        .output()
        .with_context(|| format!("Failed to execute test binary: {}", output_path.display()))?;

    // Print stdout (test results)
    if !test_result.stdout.is_empty() {
        print!("{}", String::from_utf8_lossy(&test_result.stdout));
    }

    // Print stderr (test failures)
    if !test_result.stderr.is_empty() {
        eprint!("{}", String::from_utf8_lossy(&test_result.stderr));
    }

    // Check test result
    if test_result.status.success() {
        Ok(())
    } else {
        anyhow::bail!("Tests failed");
    }
}

fn test_project() -> Result<()> {
    let current_dir = std::env::current_dir()
        .with_context(|| "Failed to get current directory")?;

    // Discover all .plat files
    let files = discover_plat_files(&current_dir)?;

    if files.is_empty() {
        anyhow::bail!("No .plat files found in current directory");
    }

    // Run tests for each file that contains test blocks
    let mut total_passed = 0;
    let mut total_failed = 0;
    let mut files_with_tests = 0;

    for file in files {
        let source = fs::read_to_string(&file)
            .with_context(|| format!("Failed to read file: {}", file.display()))?;

        let parser = plat_parser::Parser::new(&source)
            .with_context(|| "Failed to create parser")?;
        let program = parser.parse()
            .with_context(|| "Failed to parse program")?;

        if program.test_blocks.is_empty() {
            continue;
        }

        files_with_tests += 1;
        println!("\n{} {}", "Testing".green().bold(), file.display());

        match test_single_file(file) {
            Ok(_) => total_passed += 1,
            Err(_) => total_failed += 1,
        }
    }

    if files_with_tests == 0 {
        println!("{} No test blocks found", "✓".yellow().bold());
        return Ok(());
    }

    println!("\n{}", "═".repeat(50));
    println!("Test Summary: {} passed, {} failed", total_passed, total_failed);

    if total_failed > 0 {
        anyhow::bail!("Some tests failed");
    }

    Ok(())
}

/// Discover all test functions in a program
fn discover_tests(program: &plat_ast::Program) -> Vec<(String, String)> {
    let mut tests = Vec::new();

    for test_block in &program.test_blocks {
        for function in &test_block.functions {
            if function.name.starts_with("test_") {
                tests.push((test_block.name.clone(), function.name.clone()));
            }
        }
    }

    tests
}

/// Generate a test runner main function
fn generate_test_main(test_functions: &[(String, String)]) -> String {
    let mut output = String::new();

    // Generate test runner main function
    output.push_str("fn main() -> Int32 {\n");
    output.push_str("  var passed: Int32 = 0;\n");
    output.push_str("\n");

    for (test_block_name, test_func_name) in test_functions {
        output.push_str(&format!("  print(value = \"✓ {}::{}\");\n", test_block_name, test_func_name));
        output.push_str(&format!("  {}();\n", test_func_name));
        output.push_str("  passed = passed + 1;\n");
        output.push_str("\n");
    }

    output.push_str(&format!("  print(value = \"{} tests, {} passed, 0 failed\");\n", test_functions.len(), test_functions.len()));
    output.push_str("  return 0;\n");
    output.push_str("}\n");

    output
}

/// Compile a test program with test mode enabled
fn compile_test_program(program: &mut plat_ast::Program, output_path: &Path) -> Result<()> {
    // Type check with test mode enabled
    let type_checker = plat_hir::TypeChecker::new().with_test_mode();
    if let Err(e) = type_checker.check_program(program) {
        anyhow::bail!("Type checking failed: {:?}", e);
    }

    // Generate code with test mode enabled
    let codegen = plat_codegen::CodeGenerator::new()
        .with_context(|| "Failed to initialize code generator")?
        .with_test_mode();

    let object_bytes = codegen
        .generate_code(program)
        .map_err(|e| {
            eprintln!("Code generation error details: {:?}", e);
            anyhow::anyhow!("Code generation failed: {:?}", e)
        })?;

    // Write object file
    let object_file = output_path.with_extension("o");
    std::fs::write(&object_file, &object_bytes)
        .with_context(|| format!("Failed to write object file: {}", object_file.display()))?;

    // Build runtime library
    let build_result = Command::new("cargo")
        .args(&["build", "--lib", "--package", "plat-runtime"])
        .current_dir(get_project_root()?)
        .output()
        .with_context(|| "Failed to build runtime library")?;

    if !build_result.status.success() {
        anyhow::bail!(
            "Runtime library build failed: {}",
            String::from_utf8_lossy(&build_result.stderr)
        );
    }

    // Find runtime library
    let target_dir = get_project_root()?.join("target").join("debug");
    let runtime_lib = if cfg!(target_os = "macos") {
        target_dir.join("libplat_runtime.dylib")
    } else if cfg!(target_os = "windows") {
        target_dir.join("plat_runtime.dll")
    } else {
        target_dir.join("libplat_runtime.so")
    };

    // Create output directory
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create output directory: {}", parent.display()))?;
    }

    // Link
    let link_result = Command::new("cc")
        .arg("-o")
        .arg(output_path)
        .arg(&object_file)
        .arg(&runtime_lib)
        .output()
        .with_context(|| "Failed to run linker")?;

    if !link_result.status.success() {
        let stderr = String::from_utf8_lossy(&link_result.stderr);
        anyhow::bail!("Linking failed:\n{}", stderr);
    }

    // Clean up object file
    std::fs::remove_file(&object_file).ok();

    Ok(())
}

fn validate_plat_file(file: &Path) -> Result<()> {
    if !file.exists() {
        anyhow::bail!("File not found: {}", file.display());
    }

    if file.extension() != Some(std::ffi::OsStr::new("plat")) {
        anyhow::bail!("File must have .plat extension: {}", file.display());
    }

    Ok(())
}

fn get_output_path(source_file: &Path) -> PathBuf {
    let file_stem = source_file.file_stem().unwrap_or_default();
    PathBuf::from("target").join("plat").join(file_stem)
}

fn get_project_root() -> Result<PathBuf> {
    // Find the workspace root by looking for Cargo.toml with [workspace]
    let current_dir = std::env::current_dir()
        .with_context(|| "Failed to get current directory")?;

    let mut dir = current_dir.as_path();
    loop {
        let cargo_toml = dir.join("Cargo.toml");
        if cargo_toml.exists() {
            let content = std::fs::read_to_string(&cargo_toml)
                .with_context(|| format!("Failed to read {}", cargo_toml.display()))?;
            if content.contains("[workspace]") {
                return Ok(dir.to_path_buf());
            }
        }

        if let Some(parent) = dir.parent() {
            dir = parent;
        } else {
            anyhow::bail!("Could not find workspace root (Cargo.toml with [workspace])");
        }
    }
}

/// Parse a single .plat file and extract its module declaration and imports
fn parse_module_info(file_path: &Path) -> Result<(String, Vec<String>)> {
    let source = fs::read_to_string(file_path)
        .with_context(|| format!("Failed to read file: {}", file_path.display()))?;

    let parser = plat_parser::Parser::new(&source)
        .with_context(|| "Failed to create parser")?;
    let mut program = parser.parse()
        .with_context(|| "Failed to parse program")?;

    let module_path = program.module_decl
        .as_ref()
        .map(|m| m.path.join("::"))
        .unwrap_or_default();

    let imports: Vec<String> = program.use_decls
        .iter()
        .map(|u| u.path.join("::"))
        .collect();

    Ok((module_path, imports))
}

/// Discover all .plat files in the current directory tree
fn discover_plat_files(root: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();

    fn visit_dirs(dir: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
        if dir.is_dir() {
            for entry in fs::read_dir(dir)? {
                let entry = entry?;
                let path = entry.path();
                if path.is_dir() {
                    visit_dirs(&path, files)?;
                } else if path.extension() == Some(std::ffi::OsStr::new("plat")) {
                    files.push(path);
                }
            }
        }
        Ok(())
    }

    visit_dirs(root, &mut files)?;
    Ok(files)
}

/// Build module dependency graph and get compilation order
fn resolve_modules(files: &[PathBuf], root_dir: &Path) -> Result<Vec<PathBuf>> {
    let mut resolver = ModuleResolver::new(root_dir.to_path_buf());

    // Register all modules
    for file in files {
        let (module_path, imports) = parse_module_info(file)?;
        resolver.register_module(file.clone(), &module_path)
            .map_err(|e| anyhow::anyhow!("{}", e))?;
        resolver.add_dependencies(&module_path, imports);
    }

    // Get compilation order
    let order = resolver.compilation_order()
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    // Map module names back to file paths
    let mut ordered_files = Vec::new();
    for module_name in order {
        if let Ok(module_id) = resolver.resolve_module(&module_name) {
            ordered_files.push(module_id.file_path.clone());
        }
    }

    Ok(ordered_files)
}

fn bench_command(file: Option<PathBuf>) -> Result<()> {
    match file {
        Some(f) => bench_single_file(f),
        None => bench_project(),
    }
}

fn bench_single_file(file: PathBuf) -> Result<()> {
    validate_plat_file(&file)?;

    let source = fs::read_to_string(&file)
        .with_context(|| format!("Failed to read file: {}", file.display()))?;

    println!("{}", "Running benchmarks...".green().bold());

    // Parse the source code
    let parser = plat_parser::Parser::new(&source)
        .with_context(|| "Failed to create parser")?;
    let mut program = parser.parse()
        .with_context(|| "Failed to parse program")?;

    // Discover all bench functions
    let bench_functions = discover_benches(&program);

    if bench_functions.is_empty() {
        println!("{} No benchmarks found", "✓".yellow().bold());
        return Ok(());
    }

    // Generate bench runner main function
    let bench_main = generate_bench_main(&bench_functions);

    // Parse the bench main function
    let bench_main_parser = plat_parser::Parser::new(&bench_main)
        .with_context(|| "Failed to create parser for bench main")?;
    let bench_main_program = bench_main_parser.parse()
        .with_context(|| "Failed to parse bench main")?;

    // Replace or add the main function while keeping bench blocks
    if let Some(main_idx) = program.functions.iter().position(|f| f.name == "main") {
        program.functions[main_idx] = bench_main_program.functions[0].clone();
    } else {
        program.functions.push(bench_main_program.functions[0].clone());
    }

    // Compile and run the bench program
    let output_path = get_output_path(&file);
    compile_bench_program(&mut program, &output_path)?;

    // Execute the benchmarks
    let bench_result = Command::new(&output_path)
        .output()
        .with_context(|| format!("Failed to execute bench binary: {}", output_path.display()))?;

    // Print stdout (bench results)
    if !bench_result.stdout.is_empty() {
        print!("{}", String::from_utf8_lossy(&bench_result.stdout));
    }

    // Print stderr (bench failures)
    if !bench_result.stderr.is_empty() {
        eprint!("{}", String::from_utf8_lossy(&bench_result.stderr));
    }

    // Check bench result
    if bench_result.status.success() {
        Ok(())
    } else {
        anyhow::bail!("Benchmarks failed");
    }
}

fn bench_project() -> Result<()> {
    let current_dir = std::env::current_dir()
        .with_context(|| "Failed to get current directory")?;

    // Discover all .plat files
    let files = discover_plat_files(&current_dir)?;

    if files.is_empty() {
        anyhow::bail!("No .plat files found in current directory");
    }

    // Run benchmarks for each file that contains bench blocks
    let mut files_with_benches = 0;

    for file in files {
        let source = fs::read_to_string(&file)
            .with_context(|| format!("Failed to read file: {}", file.display()))?;

        let parser = plat_parser::Parser::new(&source)
            .with_context(|| "Failed to create parser")?;
        let program = parser.parse()
            .with_context(|| "Failed to parse program")?;

        if program.bench_blocks.is_empty() {
            continue;
        }

        files_with_benches += 1;
        println!("\n{} {}", "Benchmarking".green().bold(), file.display());

        bench_single_file(file)?;
    }

    if files_with_benches == 0 {
        println!("{} No bench blocks found", "✓".yellow().bold());
        return Ok(());
    }

    println!("\n{}", "═".repeat(50));
    println!("All benchmarks completed");

    Ok(())
}

/// Discover all bench functions in a program
fn discover_benches(program: &plat_ast::Program) -> Vec<(String, String)> {
    let mut benches = Vec::new();

    for bench_block in &program.bench_blocks {
        for function in &bench_block.functions {
            if function.name.starts_with("bench_") {
                benches.push((bench_block.name.clone(), function.name.clone()));
            }
        }
    }

    benches
}

/// Generate a bench runner main function
fn generate_bench_main(bench_functions: &[(String, String)]) -> String {
    let mut output = String::new();

    // Generate bench runner main function
    output.push_str("fn main() -> Int32 {\n");
    output.push_str("  let iterations = 10_000_000;\n");
    output.push_str("  let warmup_iterations = 1_000;\n");
    output.push_str("\n");

    for (idx, (bench_block_name, bench_func_name)) in bench_functions.iter().enumerate() {
        output.push_str(&format!("  print(value = \"\");\n"));
        output.push_str(&format!("  print(value = \"{}::{}\");\n", bench_block_name, bench_func_name));

        // Warmup phase - use unique variable name
        let warmup_var = format!("warmup_{}", idx);
        output.push_str(&format!("  var {} = 0;\n", warmup_var));
        output.push_str(&format!("  while ({} < warmup_iterations) {{\n", warmup_var));
        output.push_str(&format!("    {}();\n", bench_func_name));
        output.push_str(&format!("    {} = {} + 1;\n", warmup_var, warmup_var));
        output.push_str("  }\n");
        output.push_str("\n");

        // Benchmark phase - use unique variable name
        let bench_var = format!("bench_{}", idx);
        output.push_str(&format!("  var {} = 0;\n", bench_var));
        output.push_str(&format!("  while ({} < iterations) {{\n", bench_var));
        output.push_str(&format!("    {}();\n", bench_func_name));
        output.push_str(&format!("    {} = {} + 1;\n", bench_var, bench_var));
        output.push_str("  }\n");
        output.push_str(&format!("  print(value = \"  Iterations: {}\");\n", "10,000,000"));
        output.push_str(&format!("  print(value = \"  (Timing not yet implemented)\");\n"));
        output.push_str("\n");
    }

    output.push_str(&format!("  print(value = \"{} benchmarks completed\");\n", bench_functions.len()));
    output.push_str("  return 0;\n");
    output.push_str("}\n");

    output
}

/// Compile a bench program with bench mode enabled
fn compile_bench_program(program: &mut plat_ast::Program, output_path: &Path) -> Result<()> {
    // Type check with bench mode enabled
    let type_checker = plat_hir::TypeChecker::new().with_bench_mode();
    if let Err(e) = type_checker.check_program(program) {
        anyhow::bail!("Type checking failed: {:?}", e);
    }

    // Generate code with bench mode enabled
    let codegen = plat_codegen::CodeGenerator::new()
        .with_context(|| "Failed to initialize code generator")?
        .with_bench_mode();

    let object_bytes = codegen
        .generate_code(program)
        .map_err(|e| {
            eprintln!("Code generation error details: {:?}", e);
            anyhow::anyhow!("Code generation failed: {:?}", e)
        })?;

    // Write object file
    let object_file = output_path.with_extension("o");
    std::fs::write(&object_file, &object_bytes)
        .with_context(|| format!("Failed to write object file: {}", object_file.display()))?;

    // Build runtime library
    let build_result = Command::new("cargo")
        .args(&["build", "--lib", "--package", "plat-runtime"])
        .current_dir(get_project_root()?)
        .output()
        .with_context(|| "Failed to build runtime library")?;

    if !build_result.status.success() {
        anyhow::bail!(
            "Runtime library build failed: {}",
            String::from_utf8_lossy(&build_result.stderr)
        );
    }

    // Find runtime library
    let target_dir = get_project_root()?.join("target").join("debug");
    let runtime_lib = if cfg!(target_os = "macos") {
        target_dir.join("libplat_runtime.dylib")
    } else if cfg!(target_os = "windows") {
        target_dir.join("plat_runtime.dll")
    } else {
        target_dir.join("libplat_runtime.so")
    };

    // Create output directory
    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create output directory: {}", parent.display()))?;
    }

    // Link
    let link_result = Command::new("cc")
        .arg("-o")
        .arg(output_path)
        .arg(&object_file)
        .arg(&runtime_lib)
        .output()
        .with_context(|| "Failed to run linker")?;

    if !link_result.status.success() {
        let stderr = String::from_utf8_lossy(&link_result.stderr);
        anyhow::bail!("Linking failed:\n{}", stderr);
    }

    // Clean up object file
    std::fs::remove_file(&object_file).ok();

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_validate_plat_file_missing() {
        let result = validate_plat_file(Path::new("nonexistent.plat"));
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[test]
    fn test_validate_plat_file_wrong_extension() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        fs::write(&file_path, "content").unwrap();

        let result = validate_plat_file(&file_path);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains(".plat extension"));
    }

    #[test]
    fn test_validate_plat_file_success() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.plat");
        fs::write(&file_path, "content").unwrap();

        let result = validate_plat_file(&file_path);
        assert!(result.is_ok());
    }

    #[test]
    fn test_get_output_path() {
        let source = PathBuf::from("examples/hello.plat");
        let output = get_output_path(&source);
        assert_eq!(output, PathBuf::from("target/plat/hello"));

        let source = PathBuf::from("/absolute/path/program.plat");
        let output = get_output_path(&source);
        assert_eq!(output, PathBuf::from("target/plat/program"));
    }
}