
## stdlib
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




---
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
* markdown parsing 
* tasks / todo support. 
* linter 
* syntax highlighting
* config 
* any module is automatically a cli
* first-class pandas, numpy, pytorch, matrix. platnum. 
* ffi
* incremental compilation and caching 
* enforce composition over inheritance? one level of inheritance? 
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


## GC

  1. Primitive Arrays - Lists of Int32, Bool
  can also use atomic allocation (5-10% more
  gains)
  2. Incremental GC - Enable by default for
  smoother pause times
  3. Benchmark Suite - Once plat bench command
  is working, run full performance tests
