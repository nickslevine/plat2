# Beautiful Error Messages Implementation - Remaining Work Plan

## Context: What's Already Done

### Phase 1: Infrastructure (✅ Complete)
The `plat-diags` crate now provides a rich diagnostic system:

```rust
// Rich diagnostic with builder pattern
Diagnostic::syntax_error("file.plat", Span::new(10, 15), "Unexpected token")
    .with_code("E001")
    .with_label("this token is not valid here")
    .with_help("Did you forget a semicolon?")
    .with_note("Function declarations require semicolons")
    .report(&source);
```

**Available constructors:**
- `Diagnostic::syntax_error()` - Parsing/lexical errors
- `Diagnostic::type_error()` - Type checking errors
- `Diagnostic::type_mismatch()` - Expected vs actual type errors
- `Diagnostic::visibility_error()` - Private access violations
- `Diagnostic::undefined_symbol()` - Unknown identifiers
- `Diagnostic::naming_convention_error()` - snake_case/TitleCase violations
- `Diagnostic::module_error()` - Module system errors

**Key types:**
- `Diagnostic` - Rich error with spans, labels, help, notes
- `DiagnosticError` - Legacy enum with backward compatibility
- `Span` - Source location (start, end byte offsets)
- `ErrorCategory` - Lexical, Syntax, Type, Visibility, Module, Runtime

### Phase 2.1: Lexer (✅ Complete)
All lexer errors now use rich diagnostics with helpful suggestions:
- Unterminated strings
- Unexpected characters
- Invalid number literals
- Scientific notation errors

### Phase 3: CLI Integration (✅ Complete)
The CLI now reports errors beautifully via `report_diagnostic_error()`:

```rust
let parser = plat_parser::Parser::with_filename(&source, filename)
    .map_err(|e| report_diagnostic_error(e, &filename, &source))?;
```

---

## Remaining Work

### Phase 2.2: Parser Error Sites (~25 high-priority sites)

**Goal:** Convert parser errors from plain strings to rich diagnostics with helpful suggestions.

**Location:** `/Users/nlevine/Dev/plat2/crates/plat-parser/src/lib.rs`

**Current state:** Parser has ~100 error sites using legacy format:
```rust
// Current (legacy)
return Err(DiagnosticError::Syntax("Expected semicolon".to_string()));
```

**Target state:** Convert to rich diagnostics:
```rust
// Target (rich)
return Err(DiagnosticError::Rich(
    Diagnostic::syntax_error(
        &self.filename,  // Need to add this field!
        self.current_span(),
        "Expected semicolon"
    )
    .with_label("semicolon required here")
    .with_help("Add ';' after this statement")
));
```

**Steps:**

1. **Add filename tracking to Parser struct:**
   ```rust
   pub struct Parser {
       tokens: Vec<TokenWithSpan>,
       current: usize,
       filename: String,  // Add this field
   }
   ```

2. **Update Parser::new() and Parser::with_filename():**
   ```rust
   pub fn new(input: &str) -> Result<Self, DiagnosticError> {
       let lexer = Lexer::new(input);
       let tokens = lexer.tokenize()?;
       Ok(Self {
           tokens,
           current: 0,
           filename: "<unknown>".to_string(),  // Default
       })
   }

   pub fn with_filename(input: &str, filename: impl Into<String>) -> Result<Self, DiagnosticError> {
       let lexer = Lexer::with_filename(input, filename.clone());
       let tokens = lexer.tokenize()?;
       Ok(Self {
           tokens,
           current: 0,
           filename: filename.into(),
       })
   }
   ```

3. **Add helper method for current span:**
   ```rust
   fn current_span(&self) -> Span {
       if self.current < self.tokens.len() {
           self.tokens[self.current].span
       } else if self.current > 0 {
           self.tokens[self.current - 1].span
       } else {
           Span::new(0, 0)
       }
   }
   ```

4. **Priority error sites to update (start with these):**

   - **Missing semicolons** (search for: `"Expected ';'"`)
     ```rust
     Diagnostic::syntax_error(&self.filename, self.current_span(), "Expected semicolon")
         .with_label("semicolon required here")
         .with_help("Add ';' after this statement")
     ```

   - **Unexpected tokens** (search for: `"Expected"`)
     ```rust
     Diagnostic::syntax_error(
         &self.filename,
         self.current_span(),
         format!("Expected {}, found {}", expected, actual)
     )
     .with_label(format!("expected {} here", expected))
     ```

   - **Invalid patterns** (search for: `"Expected pattern"`)
   - **Invalid expressions** (search for: `"Expected expression"`)
   - **Public modifier errors** (search for: `"cannot be marked as public"`)

5. **Testing strategy:**
   ```bash
   # Run parser tests after each batch of changes
   cargo test --package plat-parser

   # Test with actual error cases
   echo 'fn main() { let x = 5 }' > /tmp/test.plat  # Missing semicolon
   cargo run --package plat-cli -- build /tmp/test.plat
   ```

6. **Commit strategy:**
   - Commit after updating each category (semicolons, tokens, patterns, etc.)
   - Include example error output in commit message

---

### Phase 2.3: Type Checker Error Sites (~50 high-priority sites)

**Goal:** Convert type checking errors to rich diagnostics with multi-label support.

**Location:** `/Users/nlevine/Dev/plat2/crates/plat-hir/src/lib.rs`

**Current state:** Type checker has ~150 error sites using legacy format:
```rust
return Err(DiagnosticError::Type(
    format!("Type mismatch: expected {}, found {}", expected, actual)
));
```

**Target state:** Use specialized constructors with multiple labels:
```rust
return Err(DiagnosticError::Rich(
    Diagnostic::type_mismatch(
        &self.filename,
        expr.span,
        expected,
        actual
    )
    .with_secondary_label(
        declaration_span,
        format!("variable declared as {} here", expected)
    )
));
```

**Steps:**

1. **Add filename tracking to TypeChecker:**
   ```rust
   pub struct TypeChecker {
       scopes: Vec<HashMap<String, HirType>>,
       // ... existing fields ...
       filename: String,  // Add this field
   }
   ```

2. **Update check_program() to accept filename:**
   ```rust
   pub fn check_program(&mut self, program: &mut Program) -> Result<(), DiagnosticError> {
       // Extract filename from first function/class span, or use default
       self.filename = "<unknown>".to_string();
       // ... rest of implementation
   }
   ```

   **Note:** You may need to update the HIR to track spans for better error reporting.

3. **Priority error categories:**

   - **Type mismatches** (search for: `"Type mismatch"`)
     ```rust
     Diagnostic::type_mismatch(&self.filename, expr.span, expected, actual)
         .with_secondary_label(decl_span, "variable declared here")
     ```

   - **Undefined variables** (search for: `"Undefined variable"`)
     ```rust
     Diagnostic::undefined_symbol(&self.filename, name_span, &name)
         .with_help("Check if the variable is in scope")
     ```

   - **Visibility violations** (search for: `"Cannot access private"`)
     ```rust
     Diagnostic::visibility_error(&self.filename, access_span, &name, "field")
         .with_secondary_label(definition_span, "field defined here as private")
     ```

   - **Naming convention violations** (search for: `"does not follow"`)
     ```rust
     Diagnostic::naming_convention_error(&self.filename, span, &name, "snake_case")
     ```

   - **Incompatible types** (search for: `"Incompatible types"`)
   - **Return type mismatches** (search for: `"Return type"`)
   - **Argument type mismatches** (search for: `"Argument type"`)

4. **Multi-label examples:**

   For function call errors, show both call site and definition:
   ```rust
   Diagnostic::type_error(
       &self.filename,
       call_span,
       format!("Function '{}' expects {} arguments, found {}", name, expected, actual)
   )
   .with_label("called here")
   .with_secondary_label(
       definition_span,
       format!("function defined with {} parameters here", expected)
   )
   .with_help(format!("Add {} more argument(s) to the call", expected - actual))
   ```

5. **Testing strategy:**
   ```bash
   # Create test files with type errors
   cat > /tmp/type_error.plat << 'EOF'
   fn main() -> Int32 {
     let x: Int32 = "hello";  # Type mismatch
     return x;
   }
   EOF

   cargo run --package plat-cli -- build /tmp/type_error.plat

   # Verify the error shows both types with helpful message
   ```

6. **Challenge: Span tracking in HIR**

   The HIR (High-level IR) may not have span information. You have two options:

   **Option A (Easier):** Use dummy spans for now:
   ```rust
   Diagnostic::type_mismatch(&self.filename, Span::new(0, 0), expected, actual)
   ```

   **Option B (Better):** Add span fields to HIR types:
   ```rust
   // In plat-ast/src/lib.rs
   pub struct Expression {
       pub kind: ExpressionKind,
       pub span: Span,  // Most expressions already have this!
   }
   ```

   Then propagate spans through type checking to create accurate error locations.

---

### Phase 4: Enhanced Error Quality

**Goal:** Add contextual help and suggestions to make errors more educational.

**Steps:**

1. **Fuzzy matching for undefined symbols:**
   ```rust
   // Add to plat-diags/Cargo.toml
   strsim = "0.11"

   // In undefined_symbol helper
   pub fn undefined_symbol_with_suggestions(
       filename: impl Into<String>,
       span: Span,
       name: impl Into<String>,
       available_symbols: &[String],
   ) -> Self {
       let name = name.into();
       let suggestions = find_similar_names(&name, available_symbols, 3);

       let mut diag = Self::undefined_symbol(filename, span, &name);

       if !suggestions.is_empty() {
           diag = diag.with_help(format!(
               "Did you mean one of these? {}",
               suggestions.join(", ")
           ));
       }

       diag
   }

   fn find_similar_names(target: &str, candidates: &[String], max: usize) -> Vec<String> {
       use strsim::jaro_winkler;

       let mut scored: Vec<_> = candidates
           .iter()
           .map(|c| (c, jaro_winkler(target, c)))
           .filter(|(_, score)| *score > 0.7)
           .collect();

       scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
       scored.into_iter().take(max).map(|(s, _)| s.clone()).collect()
   }
   ```

2. **Common error patterns with suggestions:**

   - **Missing imports:**
     ```rust
     .with_help("Add 'use module_name;' at the top of the file")
     ```

   - **Wrong case in naming:**
     ```rust
     .with_help(format!("Convert to {}: {}", convention, to_snake_case(&name)))
     ```

   - **Type conversion hints:**
     ```rust
     .with_help(format!("Use cast(value = x, target = {}) to convert", target_type))
     ```

   - **Missing pub keyword:**
     ```rust
     .with_help("Add 'pub' before this item to make it accessible from other modules")
     ```

3. **Error code documentation:**

   Create `/Users/nlevine/Dev/plat2/ERROR_CODES.md`:
   ```markdown
   # Plat Compiler Error Codes

   ## E001: Unterminated String Literal

   **Example:**
   ```plat
   let x: String = "hello
   ```

   **Fix:**
   Add a closing quote:
   ```plat
   let x: String = "hello";
   ```

   ## E002: Type Mismatch
   ...
   ```

4. **Add error codes to diagnostics:**
   ```rust
   // Update constructors to include codes
   Diagnostic::syntax_error(filename, span, message)
       .with_code("E001")

   Diagnostic::type_mismatch(filename, span, expected, actual)
       .with_code("E101")
   ```

---

## Testing Strategy

### Unit Tests
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parser_semicolon_error() {
        let source = "fn main() { let x = 5 }";
        let parser = Parser::with_filename(source, "test.plat");
        let result = parser.unwrap().parse();

        match result {
            Err(DiagnosticError::Rich(diag)) => {
                assert_eq!(diag.category, ErrorCategory::Syntax);
                assert!(diag.message.contains("semicolon"));
                assert!(diag.help.is_some());
            }
            _ => panic!("Expected rich diagnostic"),
        }
    }
}
```

### Integration Tests
```bash
# Create test suite in plat-cli/tests/error_messages.rs

#[test]
fn test_unterminated_string_error() {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    write!(tmp, "fn main() {{ let x = \"unterminated }}").unwrap();

    let output = Command::new("cargo")
        .args(&["run", "--package", "plat-cli", "--", "build", tmp.path()])
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Unterminated string"));
    assert!(stderr.contains("Help:"));
}
```

---

## Success Criteria

**Phase 2.2 Complete when:**
- [ ] Parser has filename field
- [ ] All high-priority parser errors use rich diagnostics
- [ ] All parser tests still pass
- [ ] Error messages include helpful suggestions
- [ ] At least 3 example error outputs documented

**Phase 2.3 Complete when:**
- [ ] TypeChecker has filename field
- [ ] Type mismatch errors use `type_mismatch()` constructor
- [ ] Visibility errors use `visibility_error()` constructor
- [ ] Undefined symbol errors use `undefined_symbol()` constructor
- [ ] At least 2 errors use multi-label support
- [ ] All type checker tests still pass

**Phase 4 Complete when:**
- [ ] Undefined symbols show "did you mean" suggestions
- [ ] Error codes assigned to common errors
- [ ] ERROR_CODES.md documentation created
- [ ] At least 5 error types have contextual help

---

## File Reference

**Key files to modify:**
- `/Users/nlevine/Dev/plat2/crates/plat-parser/src/lib.rs` (Phase 2.2)
- `/Users/nlevine/Dev/plat2/crates/plat-hir/src/lib.rs` (Phase 2.3)
- `/Users/nlevine/Dev/plat2/crates/plat-diags/src/lib.rs` (Phase 4 helpers)
- `/Users/nlevine/Dev/plat2/crates/plat-diags/Cargo.toml` (Phase 4 - add strsim)

**Working directory:**
```
/Users/nlevine/Dev/plat2
```

**Test commands:**
```bash
# Build all
cargo build

# Test specific crate
cargo test --package plat-parser
cargo test --package plat-hir

# Test end-to-end error messages
echo 'fn main() { let x = 5 }' > /tmp/test.plat
cargo run --package plat-cli -- build /tmp/test.plat
```

---

## Commit Strategy

Follow the existing pattern:

```bash
git add crates/plat-parser/
git commit -m "feat: Upgrade parser errors to use rich diagnostics

Convert high-priority parser error sites to use Diagnostic API:
- Add filename tracking to Parser struct
- Update semicolon errors with helpful suggestions
- Update token expectation errors with context
- Add current_span() helper for error locations

All 16 parser tests still pass.

🤖 Generated with [Claude Code](https://claude.com/claude-code)

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

## Notes

- **Incremental approach:** Don't try to convert all errors at once. Start with 5-10 high-impact errors per commit.
- **Backward compatibility:** The `DiagnosticError::Rich()` variant maintains compatibility with existing code.
- **Span availability:** AST nodes already have spans - use them! HIR nodes may need spans added.
- **Help messages:** Make them actionable. "Add ';'" is better than "Semicolon expected".
- **Multi-label usage:** Use for showing "defined here" / "used here" relationships.
