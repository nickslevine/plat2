use std::process::Command;
use std::fs;
use tempfile::TempDir;
use std::path::Path;

fn get_plat_binary() -> std::path::PathBuf {
    // Get the workspace root
    let mut current_dir = std::env::current_dir().expect("Failed to get current dir");

    // Find workspace root by looking for Cargo.toml with [workspace]
    loop {
        let cargo_toml = current_dir.join("Cargo.toml");
        if cargo_toml.exists() {
            if let Ok(content) = fs::read_to_string(&cargo_toml) {
                if content.contains("[workspace]") {
                    break;
                }
            }
        }

        if !current_dir.pop() {
            panic!("Could not find workspace root");
        }
    }

    // Build the plat binary
    let output = std::process::Command::new("cargo")
        .current_dir(&current_dir)
        .args(&["build", "--bin", "plat"])
        .output()
        .expect("Failed to build plat binary");

    if !output.status.success() {
        panic!("Failed to build plat binary: {}", String::from_utf8_lossy(&output.stderr));
    }

    // Return the path to the built binary
    let binary_path = current_dir.join("target").join("debug").join("plat");
    if !binary_path.exists() {
        panic!("Binary not found at {:?} after building", binary_path);
    }

    binary_path
}

#[test]
fn test_hello_world_execution() {
    let temp_dir = TempDir::new().unwrap();
    let source_file = temp_dir.path().join("hello.plat");

    let source = r#"
fn main() -> Int32 {
    print("Hello, World!");
    return 0;
}
"#;

    fs::write(&source_file, source).unwrap();

    let plat = get_plat_binary();
    let output = Command::new(plat)
        .arg("run")
        .arg(&source_file)
        .output()
        .expect("Failed to execute plat");

    assert!(output.status.success(), "Plat run failed: {}", String::from_utf8_lossy(&output.stderr));

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Hello, World!"), "Expected 'Hello, World!' in output, got: {}", stdout);
}

#[test]
fn test_arithmetic_operations() {
    let temp_dir = TempDir::new().unwrap();
    let source_file = temp_dir.path().join("arithmetic.plat");

    let source = r#"
fn add(a: Int32, b: Int32) -> Int32 {
    return a + b;
}

fn main() -> Int32 {
    let x = 5;
    let y = 3;
    let result = add(x, y);
    print("Math works!");
    return 0;
}
"#;

    fs::write(&source_file, source).unwrap();

    let plat = get_plat_binary();
    let output = Command::new(plat)
        .arg("run")
        .arg(&source_file)
        .output()
        .expect("Failed to execute plat");

    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Math works!"));
}

#[test]
fn test_exit_code() {
    let temp_dir = TempDir::new().unwrap();
    let source_file = temp_dir.path().join("exit.plat");

    let source = r#"
fn main() -> Int32 {
    return 42;
}
"#;

    fs::write(&source_file, source).unwrap();

    let plat = get_plat_binary();
    let output = Command::new(plat)
        .arg("run")
        .arg(&source_file)
        .output()
        .expect("Failed to execute plat");

    assert_eq!(output.status.code(), Some(42));
}

#[test]
fn test_build_command() {
    let temp_dir = TempDir::new().unwrap();
    let source_file = temp_dir.path().join("build_test.plat");

    let source = r#"
fn main() -> Int32 {
    print("Built successfully!");
    return 0;
}
"#;

    fs::write(&source_file, source).unwrap();

    let plat = get_plat_binary();
    let output = Command::new(&plat)
        .arg("build")
        .arg(&source_file)
        .output()
        .expect("Failed to execute plat build");

    assert!(output.status.success(), "Build failed: {}", String::from_utf8_lossy(&output.stderr));

    // Check that executable was created in target/plat/
    let exe_path = Path::new("target/plat/build_test");
    assert!(exe_path.exists(), "Executable not found at expected path: {}", exe_path.display());

    // Run the built executable
    let run_output = Command::new(exe_path)
        .output()
        .expect("Failed to run built executable");

    assert!(run_output.status.success());
    let stdout = String::from_utf8_lossy(&run_output.stdout);
    assert!(stdout.contains("Built successfully!"));
}

#[test]
fn test_fmt_command() {
    let temp_dir = TempDir::new().unwrap();
    let source_file = temp_dir.path().join("fmt_test.plat");

    // Badly formatted source
    let source = r#"fn   main( )  ->Int32{
print(  "Hello"  )  ;
    return   0 ;
    }"#;

    fs::write(&source_file, source).unwrap();

    let plat = get_plat_binary();
    let output = Command::new(plat)
        .arg("fmt")
        .arg(&source_file)
        .output()
        .expect("Failed to execute plat fmt");

    assert!(output.status.success());

    // Read formatted file
    let formatted = fs::read_to_string(&source_file).unwrap();

    // Check that it's properly formatted (has consistent spacing)
    assert!(formatted.contains("fn main() -> Int32"));
    assert!(formatted.contains("  print("));  // 2-space indent
}

#[test]
fn test_boolean_short_circuit() {
    let temp_dir = TempDir::new().unwrap();
    let source_file = temp_dir.path().join("boolean.plat");

    let source = r#"
fn should_not_call() -> Bool {
    print("ERROR: This should not be printed!");
    return true;
}

fn main() -> Int32 {
    print("Testing short-circuit...");
    let result = false and should_not_call();
    print("Short-circuit works!");
    return 0;
}
"#;

    fs::write(&source_file, source).unwrap();

    let plat = get_plat_binary();
    let output = Command::new(plat)
        .arg("run")
        .arg(&source_file)
        .output()
        .expect("Failed to execute plat");

    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Testing short-circuit..."));
    assert!(stdout.contains("Short-circuit works!"));
    assert!(!stdout.contains("ERROR"), "Short-circuit evaluation failed - function was called");
}

#[test]
fn test_variable_mutation() {
    let temp_dir = TempDir::new().unwrap();
    let source_file = temp_dir.path().join("mutation.plat");

    let source = r#"
fn main() -> Int32 {
    var x = 10;
    x = x + 5;
    x = x * 2;
    print("Mutation works!");
    return 0;
}
"#;

    fs::write(&source_file, source).unwrap();

    let plat = get_plat_binary();
    let output = Command::new(plat)
        .arg("run")
        .arg(&source_file)
        .output()
        .expect("Failed to execute plat");

    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Mutation works!"));
}

#[test]
fn test_string_literals() {
    let temp_dir = TempDir::new().unwrap();
    let source_file = temp_dir.path().join("strings.plat");

    let source = r#"
fn main() -> Int32 {
    print("String with spaces");
    print("String with special chars: !@#$%");
    print("");  // Empty string
    return 0;
}
"#;

    fs::write(&source_file, source).unwrap();

    let plat = get_plat_binary();
    let output = Command::new(plat)
        .arg("run")
        .arg(&source_file)
        .output()
        .expect("Failed to execute plat");

    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("String with spaces"));
    assert!(stdout.contains("String with special chars: !@#$%"));
}