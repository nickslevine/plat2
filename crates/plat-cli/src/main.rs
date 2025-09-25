use clap::{Parser, Subcommand};
use colored::*;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{self, Command};
use anyhow::{Context, Result};

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
        /// The Plat source file to build
        file: PathBuf,
    },
    /// Run a Plat source file
    Run {
        /// The Plat source file to run
        file: PathBuf,
    },
    /// Format a Plat source file
    Fmt {
        /// The Plat source file to format
        file: PathBuf,
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
    }
}

fn build_command(file: PathBuf) -> Result<()> {
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
    let program = parser.parse()
        .with_context(|| "Failed to parse program")?;

    // Type check the program
    println!("  {} Type checking...", "→".cyan());
    let type_checker = plat_hir::TypeChecker::new();
    type_checker.check_program(&program)
        .with_context(|| "Type checking failed")?;

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

fn run_command(file: PathBuf) -> Result<()> {
    validate_plat_file(&file)?;

    println!("{} {}", "Running".green().bold(), file.display());

    // First build the file
    build_command(file.clone())?;

    // Then execute the output
    let output_path = get_output_path(&file);

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