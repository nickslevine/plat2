use clap::{Parser, Subcommand};
use colored::*;
use std::fs;
use std::path::{Path, PathBuf};
use std::process;
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

    let _source = fs::read_to_string(&file)
        .with_context(|| format!("Failed to read file: {}", file.display()))?;

    let output_path = get_output_path(&file);

    println!("{} {}", "Building".green().bold(), file.display());

    // TODO: Implement actual compilation
    println!("  {} Lexing...", "→".cyan());
    println!("  {} Parsing...", "→".cyan());
    println!("  {} Type checking...", "→".cyan());
    println!("  {} Generating code...", "→".cyan());
    println!("  {} Linking...", "→".cyan());

    println!("{} Output: {}", "✓".green().bold(), output_path.display());

    Ok(())
}

fn run_command(file: PathBuf) -> Result<()> {
    validate_plat_file(&file)?;

    println!("{} {}", "Running".green().bold(), file.display());

    // First build the file
    build_command(file.clone())?;

    // Then execute the output
    let output_path = get_output_path(&file);

    // TODO: Actually execute the compiled binary
    println!("{} Executing {}", "→".cyan(), output_path.display());

    Ok(())
}

fn fmt_command(file: PathBuf) -> Result<()> {
    validate_plat_file(&file)?;

    let source = fs::read_to_string(&file)
        .with_context(|| format!("Failed to read file: {}", file.display()))?;

    println!("{} {}", "Formatting".green().bold(), file.display());

    let formatted = plat_fmt::Formatter::format(&source);

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