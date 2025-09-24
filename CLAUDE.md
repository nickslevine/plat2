```markdown
# 📜 Plat Language Implementation Plan (Rust + Cranelift)

**Goal:**  
Deliver a working `plat` CLI binary that:
- Formats, compiles, and runs `.plat` files
- Generates native executables for Linux & macOS
- Supports `bool`, `i32`, `i64`, `string` (UTF-8, GC-managed)
- Handles arithmetic, boolean logic, string interpolation
- Provides `print` built-in
- Emits clear compiler/runtime errors with **Ariadne**

---

## 0. Meta-Principles
- [ ] **TDD First**: write failing tests, make them pass, refactor.
- [ ] **No Fake Wiring**: never hard-code outputs to “look” correct.
- [ ] **Frequent Commits**: commit after each green step.
- [ ] **Plan Hygiene**: update this plan and check items as completed.

---

## 1. Project Setup
- [x] Create Cargo workspace with crates:
  - [x] `plat-cli` (main binary, CLI)
  - [x] `plat-lexer`
  - [x] `plat-parser`
  - [x] `plat-ast`
  - [x] `plat-hir` (semantic checks)
  - [x] `plat-codegen` (Cranelift backend)
  - [x] `plat-runtime` (Boehm GC bindings, builtins)
  - [x] `plat-fmt`
  - [x] `plat-diags` (Ariadne helpers)
- [x] Add dependencies:
  - [x] `clap` (CLI parsing)
  - [x] `colored` (colored terminal output)
  - [x] `ariadne` (diagnostics)
  - [x] `anyhow`, `thiserror` (error handling)
  - [x] `cranelift-codegen`, `cranelift-module`, `cranelift-object`
  - [ ] `libgc` + `boehm-rs` (FFI to Boehm GC)
  - [x] `regex` (string interpolation scanning in formatter)
- [x] Set `rust-version` (MSRV 1.77+)

---

## 2. CLI (`plat-cli`)
- [x] Define subcommands:
  - [x] `plat build <file.plat>`
  - [x] `plat run <file.plat>`
  - [x] `plat fmt <file.plat>`
- [x] Implement `target/plat/<name>` output directory logic
- [x] Add failing tests for CLI arg parsing
- [x] Make tests pass (red-green-refactor)

---

## 3. Formatter (`plat-fmt`)
- [x] Tokenize and reprint code with:
  - [x] 2-space indent
  - [x] consistent semicolons
  - [x] normalized newlines
- [x] Ensure idempotence (`fmt` run twice is a no-op)
- [x] Golden-file tests

---

## 4. Lexer (`plat-lexer`)
- [ ] Define tokens:
  - [ ] identifiers, keywords (`fn`, `let`, `var`, `true`, `false`, etc.)
  - [ ] operators (`+ - * / % and or not = == != < <= > >=`)
  - [ ] literals (`i32`, `i64`, strings with `${...}` support)
  - [ ] punctuation (`{ } ( ) ; , ->`)
- [ ] Handle Unicode string literals
- [ ] Exhaustive lex tests with Ariadne diagnostics

---

## 5. Parser (`plat-parser` + `plat-ast`)
- [x] Build recursive-descent parser for:
  - [x] Expressions (precedence climbing, C-like)
  - [x] Statements (`let`, `var`, blocks, function definitions, `print`)
  - [x] Program root
- [x] Produce AST structs (enums with spans)
- [x] Syntax error reporting via Ariadne
- [ ] Round-trip parser/formatter tests

---

## 6. Semantic Analysis (`plat-hir`)
- [x] Type checker:
  - [x] Enforce `let` immutability and no shadowing
  - [x] Allow reassignment only for `var`
  - [x] Ensure `main` exists with correct signature
- [x] Optional constant folding
- [x] Ariadne type error reporting
- [x] Unit tests for semantic errors

---

## 7. Runtime (`plat-runtime`)
- [ ] Bind to **Boehm GC**:
  - [ ] Initialize GC in `main`
  - [ ] Expose `gc_alloc`, `gc_collect`
- [x] Implement `PlatString` (UTF-8, immutable, GC heap)
- [x] Provide builtins:
  - [x] `print(PlatString)` (prints with newline)
- [x] GC stress tests (many string allocations)

---

## 8. Code Generation (`plat-codegen`)
- [ ] Integrate **Cranelift**:
  - [ ] Translate HIR to Cranelift IR
  - [ ] Emit object file and link to native executable
- [ ] Implement features:
  - [ ] Integer arithmetic
  - [ ] Boolean short-circuit (`and`, `or`)
  - [ ] Function calls and returns
  - [ ] GC-managed string allocation
  - [ ] String interpolation (`"Hello ${expr}"` → runtime concat)
- [ ] Link Boehm GC at build time
- [ ] Tests:
  - [ ] Compile & run “Hello World”
  - [ ] Compile & run arithmetic/boolean samples
  - [ ] Verify exit codes

---

## 9. End-to-End Integration
- [ ] Create sample `.plat` programs:
  - [ ] Hello world with interpolation
  - [ ] `add` function usage
  - [ ] `let`/`var` mutation
  - [ ] Boolean logic
- [ ] Integration tests running `plat run`
- [ ] Ensure executables land in `target/plat/<name>`

---

## 10. Polishing
- [ ] Rich error messages with Ariadne spans (lexer, parser, type, codegen)
- [ ] Colored CLI output (warnings/info)
- [ ] Finalize README with usage examples
- [ ] Manual tests on Linux & macOS

---

## 11. Stretch Goals (post-MVP)
- [ ] Imports & modules
- [ ] Arrays & structs
- [ ] More operators & pattern matching
- [ ] Incremental compilation & caching

---

### 🚀 Next Steps for the Agent
- [ ] Scaffold Cargo workspace and commit.
- [ ] Implement CLI skeleton with failing tests.
- [ ] Proceed down the stack: **lexer → parser → HIR → runtime → codegen**.
- [ ] After each stage: run tests, refactor, commit, and check off this plan.
```

