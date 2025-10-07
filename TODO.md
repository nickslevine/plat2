## Next
module caching
json parser syntax issue. 

 The JSON parser is 99% complete but has a syntax issue with mutable generic collections
  (var elements: List<JsonValue>) that needs investigation into Plat's type system rules.

## stdlib

* pathlib
* colored printing 
* logging package  
* json, etc
* caching
* cli/tui framework
* progress bar 
* serde etc. 
* regex
* file system operations 
* pathlib. 
* random 
* time
* datetime 
* env vars
* debugging 
* sqlite 
* ffi 
* networking
  * rate limiting, backoff, etc. 
* queue, async queue. 
* parallel loops


## Tooling
Syntax Highlighting
LSP
  show argument names
  show inferred types
  show where inherited methods come from
  fuzzy method search / autocomplete? 
Debugger. 
Linting

---

- Async/non-blocking file I/O - possibly implemented in stdlib? 
* class to_string default implementation
* automatic cli
* overflow / underflow handling
* struct (stack allocated?)?
* ✅ review gc (Phases 3 & 5: Conservative scanning works, optimization possible)
* functional programming, pipelines, lambda
* compliation speed
* plat cli niceties
  * spinner
  * ai docs
* LSP
* vs code plugin 
* docs generation 
* docs convetions / parsing. 
  * include context-efficient ai doc format. 
* markdown parsing 
* tasks / todo support. 
* linter 
* syntax highlighting
* config 
* any module is automatically a cli
* first-class pandas, numpy, pytorch, matrix. platnum. 
* ffi
* incremental compilation and caching 
* revisit verbosity. type inference, etc. 
* docs
* plat by example / the plat book
* platitudes. 
* linting / suggestions - dead code, unused imports
* central package repository
* dependency management 
* revisit type inference and required argument names 
* language code reviews. 
* iterate on error messages
* comptime? 
* js transpilation? 
* get rid of int return codes. 
* unify dot and :: syntax? 
* dive into c api / ffi
* blas
* language spec
* implementation details spec
* pre/post-conditions
* concurrency / parallel utils
* building modules efficiently / incrementally (eg depedencies, std) 
* context management 
  * tool to allow ai tools to request specific language docs context efficiently. 
* change from True to true, etc? 
* todo/ not yet implemented declaration
* allow TDD - not yet implemented compilation?
* helpful compiler messages for structured concurrency. 
* binary sizes. 
* Error types. 
* fuzz testing, parameterized testing. 
* `plat add` 
* plat check without build for speed. 
* wasm compiler/interpreter for
* change return type of main. 
* refactor stdlib. 
* ai inference library
* unify syntax (generic type params vs square brackets for List[Int], etc)




## GC

  1. Primitive Arrays - Lists of Int32, Bool
  can also use atomic allocation (5-10% more
  gains)
  2. Incremental GC - Enable by default for
  smoother pause times
  3. Benchmark Suite - Once plat bench command
  is working, run full performance tests



  📝 Key Learning

  - Plat doesn't support blocks with multiple statements in match arms - only single expressions
  - Match patterns with early returns require careful structuring to avoid type checker confusion
