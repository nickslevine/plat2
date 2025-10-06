use clap::{Parser, Subcommand};
use colored::*;
use globset::{Glob, GlobSet, GlobSetBuilder};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{self, Command};
use anyhow::{Context, Result};
use plat_modules::{ModuleResolver, ModuleError};
use plat_diags::{DiagnosticError, Span};

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
        /// Filter tests by pattern (supports glob syntax, can be specified multiple times)
        #[arg(short = 'f', long = "filter")]
        filter: Vec<String>,
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

/// Helper function to report diagnostic errors with beautiful formatting
fn report_diagnostic_error(err: DiagnosticError, filename: &str, source: &str) -> anyhow::Error {
    match err {
        DiagnosticError::Rich(diag) => {
            // Report the rich diagnostic using Ariadne
            diag.report(source);
            // Return a simple error for anyhow
            anyhow::anyhow!("Compilation failed")
        }
        // For legacy errors, convert to diagnostic and report
        _ => {
            let diag = err.to_diagnostic(filename, Span::new(0, 0));
            diag.report(source);
            anyhow::anyhow!("Compilation failed")
        }
    }
}

/// Get the standard library root directory
fn get_stdlib_root() -> PathBuf {
    // Stdlib is located in the project root directory
    // First, try to get the directory where the executable is located
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            // In development, the executable is in target/debug or target/release
            // We need to go up to the project root
            if let Some(target_dir) = exe_dir.parent() {
                if let Some(project_root) = target_dir.parent() {
                    let stdlib_path = project_root.join("stdlib");
                    if stdlib_path.exists() {
                        return stdlib_path;
                    }
                }
            }
        }
    }

    // Fallback: Try current directory + stdlib
    let current_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let stdlib_path = current_dir.join("stdlib");
    if stdlib_path.exists() {
        return stdlib_path;
    }

    // Last resort: Just return ./stdlib (will fail later if it doesn't exist)
    PathBuf::from("stdlib")
}

fn run() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Build { file } => build_command(file),
        Commands::Run { file } => run_command(file),
        Commands::Fmt { file } => fmt_command(file),
        Commands::Test { file, filter } => test_command(file, filter),
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

    let filename = file.to_string_lossy();
    let parser = plat_parser::Parser::with_filename(&source, filename.as_ref())
        .map_err(|e| report_diagnostic_error(e, &filename, &source))?;
    let program = parser.parse()
        .map_err(|e| report_diagnostic_error(e, &filename, &source))?;

    // Check if the file has imports - if so, use multi-module build
    if !program.use_decls.is_empty() {
        let current_dir = file.parent().unwrap_or_else(|| Path::new("."));

        // Discover all dependencies
        let files = vec![file.clone()];
        let ordered_files = resolve_modules(&files, current_dir)?;

        // Build all modules together
        build_multi_module(&ordered_files)?;

        println!("{} Generated executable: {}", "✓".green().bold(), output_path.display());
        return Ok(());
    }

    // No imports - use simple single-file build
    let mut program = program;

    // Type check the program
    println!("  {} Type checking...", "→".cyan());
    let type_checker = plat_hir::TypeChecker::new()
        .with_filename(filename.as_ref());
    type_checker.check_program(&mut program)
        .map_err(|e| report_diagnostic_error(e, &filename, &source))?;

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
            .with_context(|| "Failed to initialize code generator")?
            .with_symbol_table(global_symbols.clone());

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

fn test_command(file: Option<PathBuf>, filters: Vec<String>) -> Result<()> {
    match file {
        Some(f) => test_single_file(f, filters),
        None => test_project(filters),
    }
}

fn test_single_file(file: PathBuf, filters: Vec<String>) -> Result<()> {
    validate_plat_file(&file)?;

    let source = fs::read_to_string(&file)
        .with_context(|| format!("Failed to read file: {}", file.display()))?;

    println!("{}", "Running tests...".green().bold());

    // Parse the source code
    let parser = plat_parser::Parser::new(&source)
        .with_context(|| "Failed to create parser")?;
    let mut program = parser.parse()
        .with_context(|| "Failed to parse program")?;

    // Create test filter
    let filter = TestFilter::new(filters)?;

    // Get module name
    let module_name = get_module_name(&program, &file);

    // Discover all test blocks with lifecycle hooks
    let test_blocks = discover_test_blocks(&program, &filter, &module_name);

    if test_blocks.is_empty() {
        println!("0 tests, 0 passed, 0 failed");
        return Ok(());
    }

    // Generate test runner main function with hooks support
    let test_main = generate_test_main_with_hooks(&test_blocks);

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

fn test_project(filters: Vec<String>) -> Result<()> {
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
            .with_context(|| format!("Failed to create parser for file: {}", file.display()))?;
        let program = parser.parse()
            .with_context(|| format!("Failed to parse program in file: {}", file.display()))?;

        if program.test_blocks.is_empty() {
            continue;
        }

        files_with_tests += 1;
        println!("\n{} {}", "Testing".green().bold(), file.display());

        match test_single_file(file, filters.clone()) {
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

/// Information about a test block including tests and hooks
struct TestBlockInfo {
    block_name: String,
    test_functions: Vec<String>,
    has_before_each: bool,
    has_after_each: bool,
    before_each_return_type: Option<String>,
}

/// Test filter for matching tests by pattern
struct TestFilter {
    matcher: GlobSet,
    patterns: Vec<String>,
}

impl TestFilter {
    /// Create a new test filter from a list of patterns
    fn new(patterns: Vec<String>) -> Result<Self> {
        if patterns.is_empty() {
            // No filters - match everything
            let mut builder = GlobSetBuilder::new();
            builder.add(Glob::new("*")?);
            let matcher = builder.build()?;
            return Ok(Self {
                matcher,
                patterns: vec!["*".to_string()],
            });
        }

        let mut builder = GlobSetBuilder::new();
        let mut expanded_patterns = Vec::new();

        for pattern in patterns {
            // Expand shorthand patterns:
            // - Bare names (no dots, no wildcards): add .* to match everything under it
            // - Everything else: use as-is
            let expanded = if !pattern.contains('*') && !pattern.contains('.') {
                // Just a bare name: "query_tests" -> "query_tests.*"
                format!("{}.*", pattern)
            } else {
                // Keep pattern as-is - user can use explicit wildcards if needed
                pattern
            };

            builder.add(Glob::new(&expanded)?);
            expanded_patterns.push(expanded);
        }

        let matcher = builder.build()?;
        Ok(Self {
            matcher,
            patterns: expanded_patterns,
        })
    }

    /// Check if a test matches any of the filter patterns
    fn matches(&self, module: &str, test_block: &str, test_function: &str) -> bool {
        // Build various path representations to match against
        let paths_to_check = vec![
            // Full path with module
            if module.is_empty() {
                format!("{}.{}", test_block, test_function)
            } else {
                format!("{}.{}.{}", module, test_block, test_function)
            },
            // Module + test block
            if module.is_empty() {
                test_block.to_string()
            } else {
                format!("{}.{}", module, test_block)
            },
            // Just module (if not empty)
            module.to_string(),
            // Test block + test function (without module for convenience)
            format!("{}.{}", test_block, test_function),
            // Just test block (for matching all tests in a block)
            test_block.to_string(),
        ];

        // Check if any path matches any filter pattern
        paths_to_check.iter().any(|path| {
            !path.is_empty() && self.matcher.is_match(path)
        })
    }

    /// Check if we have any filters (not just the default "*" matcher)
    fn has_filters(&self) -> bool {
        self.patterns.len() > 1 || (self.patterns.len() == 1 && self.patterns[0] != "*")
    }
}

/// Extract the module name from a program
fn get_module_name(program: &plat_ast::Program, file_path: &Path) -> String {
    program.module_decl
        .as_ref()
        .map(|m| m.path.join("::"))
        .unwrap_or_else(|| {
            // Fall back to filename without extension
            file_path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string()
        })
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

/// Discover all test blocks with their functions and lifecycle hooks
fn discover_test_blocks(program: &plat_ast::Program, filter: &TestFilter, module_name: &str) -> Vec<TestBlockInfo> {
    let mut test_blocks = Vec::new();

    for test_block in &program.test_blocks {
        let mut test_functions = Vec::new();
        let mut has_before_each = false;
        let mut has_after_each = false;
        let mut before_each_return_type: Option<String> = None;

        for function in &test_block.functions {
            if function.name == "before_each" {
                has_before_each = true;
                // Extract return type from function signature
                if let Some(return_type) = &function.return_type {
                    before_each_return_type = Some(type_to_string(return_type));
                }
            } else if function.name == "after_each" {
                has_after_each = true;
            } else if function.name.starts_with("test_") {
                // Apply filter
                if filter.matches(module_name, &test_block.name, &function.name) {
                    test_functions.push(function.name.clone());
                }
            }
        }

        // Only include test blocks that have matching test functions after filtering
        if !test_functions.is_empty() {
            test_blocks.push(TestBlockInfo {
                block_name: test_block.name.clone(),
                test_functions,
                has_before_each,
                has_after_each,
                before_each_return_type,
            });
        }
    }

    test_blocks
}

/// Convert a Type to a string representation for code generation
fn type_to_string(ty: &plat_ast::Type) -> String {
    match ty {
        plat_ast::Type::Int32 => "Int32".to_string(),
        plat_ast::Type::Int64 => "Int64".to_string(),
        plat_ast::Type::Int8 => "Int8".to_string(),
        plat_ast::Type::Int16 => "Int16".to_string(),
        plat_ast::Type::Float32 => "Float32".to_string(),
        plat_ast::Type::Float64 => "Float64".to_string(),
        plat_ast::Type::Float8 => "Float8".to_string(),
        plat_ast::Type::Float16 => "Float16".to_string(),
        plat_ast::Type::Bool => "Bool".to_string(),
        plat_ast::Type::String => "String".to_string(),
        plat_ast::Type::List(inner) => format!("List[{}]", type_to_string(inner)),
        plat_ast::Type::Dict(key, value) => {
            format!("Dict[{}, {}]", type_to_string(key), type_to_string(value))
        }
        plat_ast::Type::Set(inner) => format!("Set[{}]", type_to_string(inner)),
        plat_ast::Type::Named(name, params) => {
            if params.is_empty() {
                name.clone()
            } else {
                let params_str = params.iter().map(|p| type_to_string(p)).collect::<Vec<_>>().join(", ");
                format!("{}<{}>", name, params_str)
            }
        }
    }
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

/// Generate a test runner main function with lifecycle hooks support
fn generate_test_main_with_hooks(test_blocks: &[TestBlockInfo]) -> String {
    let mut output = String::new();

    // Count total tests
    let total_tests: usize = test_blocks.iter().map(|tb| tb.test_functions.len()).sum();

    // Generate test runner main function
    output.push_str("fn main() -> Int32 {\n");

    // Handle empty test list
    if total_tests == 0 {
        output.push_str("  print(value = \"0 tests, 0 passed, 0 failed\");\n");
        output.push_str("  return 0;\n");
        output.push_str("}\n");
        return output;
    }

    output.push_str("  var passed: Int32 = 0;\n");
    output.push_str("  var failed: Int32 = 0;\n");
    output.push_str("\n");

    let mut test_idx = 0;
    for test_block in test_blocks {
        for test_func_name in &test_block.test_functions {
            // Reset test failure flag before each test
            output.push_str("  __test_reset();\n");

            // Call before_each if it exists
            if test_block.has_before_each {
                if let Some(return_type) = &test_block.before_each_return_type {
                    output.push_str(&format!("  let ctx_{}: {} = before_each();\n", test_idx, return_type));
                } else {
                    // Fallback if no return type found (shouldn't happen)
                    output.push_str(&format!("  let ctx_{} = before_each();\n", test_idx));
                }
            }

            // Call the test function with context if before_each exists
            if test_block.has_before_each {
                output.push_str(&format!("  {}(ctx = ctx_{});\n", test_func_name, test_idx));
            } else {
                output.push_str(&format!("  {}();\n", test_func_name));
            }

            // Call after_each if it exists
            if test_block.has_after_each {
                if test_block.has_before_each {
                    output.push_str(&format!("  after_each(ctx = ctx_{});\n", test_idx));
                } else {
                    // Error: after_each without before_each doesn't make sense
                    // For now, we'll just skip it
                }
            }

            // Check if test failed and update counters
            output.push_str(&format!("  let test_failed_{}: Bool = __test_check();\n", test_idx));
            output.push_str(&format!("  if (test_failed_{}) {{\n", test_idx));
            output.push_str(&format!("    print(value = \"✗ {}::{}\");\n", test_block.block_name, test_func_name));
            output.push_str("    failed = failed + 1;\n");
            output.push_str("  } else {\n");
            output.push_str(&format!("    print(value = \"✓ {}::{}\");\n", test_block.block_name, test_func_name));
            output.push_str("    passed = passed + 1;\n");
            output.push_str("  }\n");
            output.push_str("\n");

            test_idx += 1;
        }
    }

    output.push_str(&format!("  print(value = \"{} tests, ${{passed}} passed, ${{failed}} failed\");\n", total_tests));
    output.push_str("  if (failed > 0) {\n");
    output.push_str("    return 1;\n");
    output.push_str("  }\n");
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
    let stdlib_dir = get_stdlib_root();
    let mut resolver = ModuleResolver::new(root_dir.to_path_buf(), stdlib_dir);

    // Register all user modules
    for file in files {
        let (module_path, imports) = parse_module_info(file)?;
        resolver.register_module(file.clone(), &module_path)
            .map_err(|e| anyhow::anyhow!("{}", e))?;
        resolver.add_dependencies(&module_path, imports);
    }

    // Discover and register stdlib modules that are imported
    // We need to do this in a loop because stdlib modules might import other stdlib modules
    let mut processed_modules = std::collections::HashSet::new();
    let mut to_process: Vec<PathBuf> = files.to_vec();

    while !to_process.is_empty() {
        let file = to_process.remove(0);
        if processed_modules.contains(&file) {
            continue;
        }
        processed_modules.insert(file.clone());

        let (_, imports) = parse_module_info(&file)?;

        // For each import that starts with std::, discover and register it
        for import in imports.iter() {
            if import.starts_with("std::") {
                // Try to discover the stdlib module
                if let Ok(module_id) = resolver.discover_stdlib_module(&import) {
                    // Add the stdlib module's dependencies
                    let (stdlib_module_path, stdlib_imports) = parse_module_info(&module_id.file_path)?;
                    resolver.add_dependencies(&stdlib_module_path, stdlib_imports);

                    // Also process this stdlib file for its imports
                    to_process.push(module_id.file_path.clone());
                }
            }
        }
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
    output.push_str("  let iterations: Int32 = 10_000_000;\n");
    output.push_str("  let warmup_iterations: Int32 = 1_000;\n");
    output.push_str("\n");

    for (idx, (bench_block_name, bench_func_name)) in bench_functions.iter().enumerate() {
        output.push_str(&format!("  print(value = \"\");\n"));
        output.push_str(&format!("  print(value = \"{}::{}\");\n", bench_block_name, bench_func_name));

        // Warmup phase - use unique variable name
        let warmup_var = format!("warmup_{}", idx);
        output.push_str(&format!("  var {}: Int32 = 0;\n", warmup_var));
        output.push_str(&format!("  while ({} < warmup_iterations) {{\n", warmup_var));
        output.push_str(&format!("    {}();\n", bench_func_name));
        output.push_str(&format!("    {} = {} + 1;\n", warmup_var, warmup_var));
        output.push_str("  }\n");
        output.push_str("\n");

        // Benchmark phase - use unique variable name
        let bench_var = format!("bench_{}", idx);
        output.push_str(&format!("  var {}: Int32 = 0;\n", bench_var));
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