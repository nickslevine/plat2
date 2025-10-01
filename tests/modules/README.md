# Module System Tests

## Current Status (Phase 1 & 2 Complete)

### ✅ Working Features
- Module declarations: `mod math;`
- Import statements: `use math;`
- Module path validation (folder structure enforcement)
- Dependency graph construction
- Circular dependency detection
- Compilation order resolution
- Single-file programs with module declarations

### 🚧 In Progress (Phase 3 - Multi-Module Linking)
- Qualified function calls across modules (e.g., `math::add`)
- Multi-module compilation and linking
- Cross-module symbol resolution

## Test Files

### `simple_module.plat`
Single-file program with module declaration. **WORKING**

### `math.plat`, `utils.plat`, `main.plat`
Multi-file project demonstrating module dependencies. **Requires Phase 3 implementation**

## Next Steps for Full Module Support

1. **Update TypeChecker for Cross-Module Resolution**
   - When checking a program, load and register symbols from imported modules
   - Build a global symbol table across all modules in the dependency graph
   - Resolve qualified function calls (module::function)

2. **Update CodeGen for Multi-Module Linking**
   - Generate object files for each module
   - Link multiple object files together
   - Export/import symbols across module boundaries
   - Handle qualified function names in code generation

3. **CLI Integration**
   - `plat build` (no args) should discover and compile all modules
   - `plat build file.plat` should compile that file + its dependencies
   - `plat run` (no args) should look for main.plat

## Design Notes

**Module Path Validation**: The compiler enforces that `mod database::connection;` must be in `database/connection.plat`. This enables fast compilation without scanning the entire project.

**Compilation Modes**:
- Single-file: `plat run file.plat` → compile only that file
- Multi-file: `plat build` → compile all .plat files in dependency order
- Dependency-driven: `plat build main.plat` → compile main.plat + follow its `use` statements
