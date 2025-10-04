use plat_codegen::CodeGenerator;
use plat_parser::Parser;
use plat_hir::TypeChecker;
use std::process::Command;
use std::fs;
use tempfile::TempDir;

#[test]
fn test_hello_world_compilation() {
    let source = r#"
fn main() -> Int32 {
    print(value = "Hello, World!");
    return 0;
}
"#;

    // Parse
    let parser = Parser::new(source).unwrap();
    let program = parser.parse().unwrap();

    // Type check
    let type_checker = TypeChecker::new();
    type_checker.check_program(&program).unwrap();

    // Generate code
    let codegen = CodeGenerator::new().unwrap();
    let object_bytes = codegen.generate_code(&program).unwrap();

    // Object file should be non-empty
    assert!(!object_bytes.is_empty());
}

#[test]
fn test_arithmetic_compilation() {
    let source = r#"
fn add(a: Int32, b: Int32) -> Int32 {
    return a + b;
}

fn main() -> Int32 {
    let x: Int32 = 10;
    let y: Int32 = 20;
    let result: Int32 = add(a = x, b = y);
    return result;
}
"#;

    // Parse
    let parser = Parser::new(source).unwrap();
    let program = parser.parse().unwrap();

    // Type check
    let type_checker = TypeChecker::new();
    type_checker.check_program(&program).unwrap();

    // Generate code
    let codegen = CodeGenerator::new().unwrap();
    let object_bytes = codegen.generate_code(&program).unwrap();

    assert!(!object_bytes.is_empty());
}

#[test]
fn test_boolean_short_circuit_compilation() {
    let source = r#"
fn main() -> Int32 {
    let a: Bool = true;
    let b: Bool = false;
    let result: Bool = a and b;
    let short: Bool = false and a;
    return 0;
}
"#;

    // Parse
    let parser = Parser::new(source).unwrap();
    let program = parser.parse().unwrap();

    // Type check
    let type_checker = TypeChecker::new();
    type_checker.check_program(&program).unwrap();

    // Generate code
    let codegen = CodeGenerator::new().unwrap();
    let object_bytes = codegen.generate_code(&program).unwrap();

    assert!(!object_bytes.is_empty());
}

#[test]
fn test_string_interpolation_compilation() {
    let source = r#"
fn main() -> Int32 {
    let name: String = "World";
    print(value = "Hello, ${name}!");
    return 0;
}
"#;

    // Parse
    let parser = Parser::new(source).unwrap();
    let program = parser.parse().unwrap();

    // Type check
    let type_checker = TypeChecker::new();
    type_checker.check_program(&program).unwrap();

    // Generate code
    let codegen = CodeGenerator::new().unwrap();
    let object_bytes = codegen.generate_code(&program).unwrap();

    assert!(!object_bytes.is_empty());
}

// This test actually tries to compile and run a simple program
#[test]
#[ignore] // Ignore by default as it requires linking
fn test_end_to_end_execution() {
    let source = r#"
fn main() -> Int32 {
    return 42;
}
"#;

    // Parse
    let parser = Parser::new(source).unwrap();
    let program = parser.parse().unwrap();

    // Type check
    let type_checker = TypeChecker::new();
    type_checker.check_program(&program).unwrap();

    // Generate code
    let codegen = CodeGenerator::new().unwrap();
    let object_bytes = codegen.generate_code(&program).unwrap();

    // Write to temp file and link
    let temp_dir = TempDir::new().unwrap();
    let object_file = temp_dir.path().join("test.o");
    let exe_file = temp_dir.path().join("test");

    fs::write(&object_file, object_bytes).unwrap();

    // Link (simplified - in reality we'd need runtime library)
    let link_result = Command::new("cc")
        .arg("-o")
        .arg(&exe_file)
        .arg(&object_file)
        .output();

    if let Ok(output) = link_result {
        if output.status.success() {
            // Run the executable
            let run_result = Command::new(&exe_file).output().unwrap();
            assert_eq!(run_result.status.code(), Some(42));
        }
    }
}