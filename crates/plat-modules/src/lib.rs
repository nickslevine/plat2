//! Module resolution system for Plat
//!
//! This crate handles:
//! - Module declaration and import validation
//! - Dependency graph construction
//! - Circular dependency detection
//! - Module path resolution based on folder structure

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

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
    /// Map of module paths to their metadata
    modules: HashMap<String, ModuleId>,
    /// Dependency graph
    dependencies: HashMap<String, Vec<String>>,
}

impl ModuleResolver {
    pub fn new(root_dir: PathBuf) -> Self {
        Self {
            root_dir,
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

    /// Resolve a module path to its file location
    pub fn resolve_module(&self, module_path: &str) -> Result<&ModuleId, ModuleError> {
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
        let mut resolver = ModuleResolver::new(PathBuf::from("/project"));

        let result = resolver.register_module(
            PathBuf::from("/project/database/connection.plat"),
            "database::connection",
        );

        assert!(result.is_ok());
    }

    #[test]
    fn test_path_mismatch() {
        let mut resolver = ModuleResolver::new(PathBuf::from("/project"));

        let result = resolver.register_module(
            PathBuf::from("/project/utils/helper.plat"),
            "database::connection",
        );

        assert!(matches!(result, Err(ModuleError::PathMismatch { .. })));
    }

    #[test]
    fn test_circular_dependency_detection() {
        let mut resolver = ModuleResolver::new(PathBuf::from("/project"));

        resolver.register_module(PathBuf::from("/project/a.plat"), "a").unwrap();
        resolver.register_module(PathBuf::from("/project/b.plat"), "b").unwrap();

        resolver.add_dependencies("a", vec!["b".to_string()]);
        resolver.add_dependencies("b", vec!["a".to_string()]);

        let result = resolver.check_circular_dependencies();
        assert!(matches!(result, Err(ModuleError::CircularDependency { .. })));
    }

    #[test]
    fn test_compilation_order() {
        let mut resolver = ModuleResolver::new(PathBuf::from("/project"));

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
