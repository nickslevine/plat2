//! Module resolution system for Plat
//!
//! This crate handles:
//! - Module declaration and import validation
//! - Dependency graph construction
//! - Circular dependency detection
//! - Module path resolution based on folder structure
//! - Object file caching for stdlib modules

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::fs;
use std::time::SystemTime;

/// Represents a module's identity and metadata
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ModuleId {
    /// Full module path (e.g., "database::connection")
    pub path: String,
    /// Physical file path
    pub file_path: PathBuf,
}

/// Represents a module's dependencies
#[derive(Debug, Clone)]
pub struct ModuleDependencies {
    pub id: ModuleId,
    /// Modules imported via `use` statements
    pub imports: Vec<String>,
}

/// Module resolution errors
#[derive(Debug, Clone)]
pub enum ModuleError {
    /// Module declaration doesn't match file location
    PathMismatch {
        declared: String,
        expected: String,
        file_path: PathBuf,
    },
    /// Circular dependency detected
    CircularDependency {
        cycle: Vec<String>,
    },
    /// Module not found
    ModuleNotFound {
        module_path: String,
        searched_paths: Vec<PathBuf>,
    },
    /// Duplicate definition within module
    DuplicateDefinition {
        module_path: String,
        item_name: String,
        locations: Vec<PathBuf>,
    },
}

impl std::fmt::Display for ModuleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ModuleError::PathMismatch { declared, expected, file_path } => {
                write!(
                    f,
                    "Module declaration '{}' doesn't match file location. Expected '{}' for file: {}",
                    declared, expected, file_path.display()
                )
            }
            ModuleError::CircularDependency { cycle } => {
                write!(f, "Circular dependency detected: {}", cycle.join(" -> "))
            }
            ModuleError::ModuleNotFound { module_path, searched_paths } => {
                write!(
                    f,
                    "Module '{}' not found. Searched paths: {}",
                    module_path,
                    searched_paths.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join(", ")
                )
            }
            ModuleError::DuplicateDefinition { module_path, item_name, locations } => {
                write!(
                    f,
                    "Duplicate definition of '{}' in module '{}'. Found in: {}",
                    item_name,
                    module_path,
                    locations.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join(", ")
                )
            }
        }
    }
}

impl std::error::Error for ModuleError {}

/// Module resolver that builds dependency graphs
pub struct ModuleResolver {
    /// Root directory for module resolution
    root_dir: PathBuf,
    /// Standard library root directory (for std:: modules)
    stdlib_dir: PathBuf,
    /// Map of module paths to their metadata
    modules: HashMap<String, ModuleId>,
    /// Dependency graph
    dependencies: HashMap<String, Vec<String>>,
}

impl ModuleResolver {
    pub fn new(root_dir: PathBuf, stdlib_dir: PathBuf) -> Self {
        Self {
            root_dir,
            stdlib_dir,
            modules: HashMap::new(),
            dependencies: HashMap::new(),
        }
    }

    /// Register a module from its file path and declared module path
    pub fn register_module(
        &mut self,
        file_path: PathBuf,
        declared_path: &str,
    ) -> Result<ModuleId, ModuleError> {
        // Check if this is a stdlib module
        if declared_path.starts_with("std::") {
            return self.register_stdlib_module(file_path, declared_path);
        }

        // Validate that module path matches file location
        let expected_path = self.file_path_to_module_path(&file_path)?;

        if declared_path != expected_path && !declared_path.is_empty() {
            return Err(ModuleError::PathMismatch {
                declared: declared_path.to_string(),
                expected: expected_path.clone(),
                file_path: file_path.clone(),
            });
        }

        let module_id = ModuleId {
            path: declared_path.to_string(),
            file_path: file_path.clone(),
        };

        self.modules.insert(declared_path.to_string(), module_id.clone());
        Ok(module_id)
    }

    /// Register a standard library module
    fn register_stdlib_module(
        &mut self,
        file_path: PathBuf,
        declared_path: &str,
    ) -> Result<ModuleId, ModuleError> {
        // For stdlib modules, validate against stdlib_dir instead of root_dir
        let relative = file_path
            .strip_prefix(&self.stdlib_dir)
            .map_err(|_| ModuleError::PathMismatch {
                declared: declared_path.to_string(),
                expected: format!("std::*"),
                file_path: file_path.clone(),
            })?;

        // Convert file path to module path
        // The relative path is something like "std/hello.plat", we want "std::hello"
        let mut components: Vec<String> = relative
            .parent()
            .unwrap_or(Path::new(""))
            .components()
            .filter_map(|c| c.as_os_str().to_str().map(|s| s.to_string()))
            .collect();

        // Add filename without extension
        if let Some(stem) = relative.file_stem() {
            if let Some(name) = stem.to_str() {
                if name != "main" {
                    components.push(name.to_string());
                }
            }
        }

        let expected_path = components.join("::");

        if declared_path != expected_path {
            return Err(ModuleError::PathMismatch {
                declared: declared_path.to_string(),
                expected: expected_path,
                file_path: file_path.clone(),
            });
        }

        let module_id = ModuleId {
            path: declared_path.to_string(),
            file_path: file_path.clone(),
        };

        self.modules.insert(declared_path.to_string(), module_id.clone());
        Ok(module_id)
    }

    /// Add dependencies for a module
    pub fn add_dependencies(&mut self, module_path: &str, imports: Vec<String>) {
        self.dependencies.insert(module_path.to_string(), imports);
    }

    /// Resolve module path based on file location
    fn file_path_to_module_path(&self, file_path: &Path) -> Result<String, ModuleError> {
        let relative = file_path
            .strip_prefix(&self.root_dir)
            .map_err(|_| ModuleError::PathMismatch {
                declared: String::new(),
                expected: String::new(),
                file_path: file_path.to_path_buf(),
            })?;

        let mut components: Vec<String> = relative
            .parent()
            .unwrap_or(Path::new(""))
            .components()
            .filter_map(|c| c.as_os_str().to_str().map(|s| s.to_string()))
            .collect();

        // Add filename without extension as last component
        if let Some(stem) = relative.file_stem() {
            if let Some(name) = stem.to_str() {
                if name != "main" {
                    components.push(name.to_string());
                }
            }
        }

        Ok(components.join("::"))
    }

    /// Check for circular dependencies using DFS
    pub fn check_circular_dependencies(&self) -> Result<(), ModuleError> {
        let mut visited = HashSet::new();
        let mut rec_stack = HashSet::new();
        let mut path = Vec::new();

        for module in self.modules.keys() {
            if !visited.contains(module) {
                self.dfs_cycle_check(module, &mut visited, &mut rec_stack, &mut path)?;
            }
        }

        Ok(())
    }

    fn dfs_cycle_check(
        &self,
        module: &str,
        visited: &mut HashSet<String>,
        rec_stack: &mut HashSet<String>,
        path: &mut Vec<String>,
    ) -> Result<(), ModuleError> {
        visited.insert(module.to_string());
        rec_stack.insert(module.to_string());
        path.push(module.to_string());

        if let Some(deps) = self.dependencies.get(module) {
            for dep in deps {
                if !visited.contains(dep) {
                    self.dfs_cycle_check(dep, visited, rec_stack, path)?;
                } else if rec_stack.contains(dep) {
                    // Found cycle
                    let cycle_start = path.iter().position(|m| m == dep).unwrap();
                    let mut cycle = path[cycle_start..].to_vec();
                    cycle.push(dep.clone());
                    return Err(ModuleError::CircularDependency { cycle });
                }
            }
        }

        path.pop();
        rec_stack.remove(module);
        Ok(())
    }

    /// Get compilation order (topological sort)
    pub fn compilation_order(&self) -> Result<Vec<String>, ModuleError> {
        self.check_circular_dependencies()?;

        let mut order = Vec::new();
        let mut visited = HashSet::new();

        for module in self.modules.keys() {
            if !visited.contains(module) {
                self.topological_sort(module, &mut visited, &mut order);
            }
        }

        // Don't reverse - we want dependencies first
        Ok(order)
    }

    fn topological_sort(&self, module: &str, visited: &mut HashSet<String>, order: &mut Vec<String>) {
        visited.insert(module.to_string());

        // Visit dependencies first
        if let Some(deps) = self.dependencies.get(module) {
            for dep in deps {
                if !visited.contains(dep) {
                    self.topological_sort(dep, visited, order);
                }
            }
        }

        // Add current module after all dependencies
        order.push(module.to_string());
    }

    /// Discover and register a stdlib module on-demand
    pub fn discover_stdlib_module(&mut self, module_path: &str) -> Result<ModuleId, ModuleError> {
        if !module_path.starts_with("std::") {
            return Err(ModuleError::ModuleNotFound {
                module_path: module_path.to_string(),
                searched_paths: vec![self.root_dir.clone()],
            });
        }

        // Convert module path to file path
        // e.g., "std::json" -> "stdlib/std/json.plat"
        let parts: Vec<&str> = module_path.split("::").collect();
        let mut file_path = self.stdlib_dir.clone();

        for part in &parts {
            file_path.push(part);
        }
        file_path.set_extension("plat");

        // Check if file exists
        if !file_path.exists() {
            return Err(ModuleError::ModuleNotFound {
                module_path: module_path.to_string(),
                searched_paths: vec![self.stdlib_dir.clone(), file_path],
            });
        }

        // Register the module
        self.register_stdlib_module(file_path, module_path)
    }

    /// Resolve a module path to its file location
    pub fn resolve_module(&mut self, module_path: &str) -> Result<&ModuleId, ModuleError> {
        // If not already registered and starts with std::, try to discover it
        if !self.modules.contains_key(module_path) && module_path.starts_with("std::") {
            self.discover_stdlib_module(module_path)?;
        }

        self.modules.get(module_path).ok_or_else(|| {
            ModuleError::ModuleNotFound {
                module_path: module_path.to_string(),
                searched_paths: vec![self.root_dir.clone()],
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_register_module() {
        let mut resolver = ModuleResolver::new(
            PathBuf::from("/project"),
            PathBuf::from("/stdlib")
        );

        let result = resolver.register_module(
            PathBuf::from("/project/database/connection.plat"),
            "database::connection",
        );

        assert!(result.is_ok());
    }

    #[test]
    fn test_path_mismatch() {
        let mut resolver = ModuleResolver::new(
            PathBuf::from("/project"),
            PathBuf::from("/stdlib")
        );

        let result = resolver.register_module(
            PathBuf::from("/project/utils/helper.plat"),
            "database::connection",
        );

        assert!(matches!(result, Err(ModuleError::PathMismatch { .. })));
    }

    #[test]
    fn test_circular_dependency_detection() {
        let mut resolver = ModuleResolver::new(
            PathBuf::from("/project"),
            PathBuf::from("/stdlib")
        );

        resolver.register_module(PathBuf::from("/project/a.plat"), "a").unwrap();
        resolver.register_module(PathBuf::from("/project/b.plat"), "b").unwrap();

        resolver.add_dependencies("a", vec!["b".to_string()]);
        resolver.add_dependencies("b", vec!["a".to_string()]);

        let result = resolver.check_circular_dependencies();
        assert!(matches!(result, Err(ModuleError::CircularDependency { .. })));
    }

    #[test]
    fn test_compilation_order() {
        let mut resolver = ModuleResolver::new(
            PathBuf::from("/project"),
            PathBuf::from("/stdlib")
        );

        resolver.register_module(PathBuf::from("/project/a.plat"), "a").unwrap();
        resolver.register_module(PathBuf::from("/project/b.plat"), "b").unwrap();
        resolver.register_module(PathBuf::from("/project/c.plat"), "c").unwrap();

        // b depends on a, c depends on b
        resolver.add_dependencies("b", vec!["a".to_string()]);
        resolver.add_dependencies("c", vec!["b".to_string()]);
        resolver.add_dependencies("a", vec![]);

        let order = resolver.compilation_order().unwrap();

        println!("Compilation order: {:?}", order);

        // a should come before b, b before c
        let a_pos = order.iter().position(|m| m == "a").unwrap();
        let b_pos = order.iter().position(|m| m == "b").unwrap();
        let c_pos = order.iter().position(|m| m == "c").unwrap();

        assert!(a_pos < b_pos, "a at {}, b at {}", a_pos, b_pos);
        assert!(b_pos < c_pos, "b at {}, c at {}", b_pos, c_pos);
    }
}

/// Cache for compiled stdlib modules
/// Uses object file caching: stores compiled .o files and checks timestamps
pub struct StdlibCache {
    cache_dir: PathBuf,
}

impl StdlibCache {
    /// Create a new cache instance
    pub fn new(cache_dir: PathBuf) -> Self {
        Self { cache_dir }
    }

    /// Initialize the cache directory structure
    pub fn init(&self) -> std::io::Result<()> {
        fs::create_dir_all(&self.cache_dir)?;
        Ok(())
    }

    /// Get the cache file path for a module
    fn cache_path(&self, module_path: &str) -> PathBuf {
        // Convert module path to safe filename
        // e.g., "std::json" -> "std-json.o"
        let safe_name = module_path.replace("::", "-");
        self.cache_dir.join(format!("{}.o", safe_name))
    }

    /// Check if a cached object file exists and is up-to-date
    pub fn is_cached(&self, module_path: &str, source_path: &Path) -> bool {
        let cache_path = self.cache_path(module_path);

        if !cache_path.exists() {
            return false;
        }

        // Compare modification times
        let cache_modified = match fs::metadata(&cache_path).and_then(|m| m.modified()) {
            Ok(time) => time,
            Err(_) => return false,
        };

        let source_modified = match fs::metadata(source_path).and_then(|m| m.modified()) {
            Ok(time) => time,
            Err(_) => return false,
        };

        // Cache is valid if it's newer than source
        cache_modified > source_modified
    }

    /// Get the path to a cached object file (if valid)
    pub fn get(&self, module_path: &str, source_path: &Path) -> Option<PathBuf> {
        if self.is_cached(module_path, source_path) {
            Some(self.cache_path(module_path))
        } else {
            None
        }
    }

    /// Store a compiled object file in the cache
    pub fn put(&self, module_path: &str, object_file: &Path) -> std::io::Result<()> {
        let cache_path = self.cache_path(module_path);
        fs::copy(object_file, cache_path)?;
        Ok(())
    }

    /// Invalidate (delete) a cached module
    pub fn invalidate(&self, module_path: &str) -> std::io::Result<()> {
        let cache_path = self.cache_path(module_path);
        if cache_path.exists() {
            fs::remove_file(cache_path)?;
        }
        Ok(())
    }

    /// Clear all cached modules
    pub fn clear_all(&self) -> std::io::Result<()> {
        if self.cache_dir.exists() {
            for entry in fs::read_dir(&self.cache_dir)? {
                let entry = entry?;
                if entry.path().extension().and_then(|s| s.to_str()) == Some("o") {
                    fs::remove_file(entry.path())?;
                }
            }
        }
        Ok(())
    }
}
