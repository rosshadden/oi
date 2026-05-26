# name
- Polly?
# facets
- trailing struct literals `v`
- strongly & statically typed `rust v go zig`
- everything is an expression `rust revo`
- pattern matching `rust haskell`
- implicit return `rust`
- opt-in mutable `rust v`
- pipes `Nushell revo`
- keywords `clojure revo`
- optional types (`?`) `v zig`
- result types (`!`) `v`
- structs `c v go rust`
- comptime `zig revo nim`
- no parens needed for most expressions `v rust go`
- `<compiler> fmt` `v go`
- generics `rust v`
- metaprogramming
- block expressions `rust`
- compound types `rust`
- macros `rust revo`
- multiple returns `v lua`
- enum literals `zig v`
- doc comments `zig v rust`
- first-class
	- `assert` `rust v zig`
	- `test` (and also `suite` and `test/skip`) `zig revo`
	- `build` `zig revo`
	- docgen `rust v go`
# syntax
- `fn`
- `const`
- free-form / C-like / whitespace insignificant
- `mut`
- `:=`
- no semicolons
- `.enum_literal`
```
```
# influences
- V
- Zig
- [revo](https://github.com/if-not-nil/revo)
- Clojure
	- metaprogramming
	- homoiconicity
- Nushell
	- structured data pipelines
- Rust
	- `impl`
- Nim
- Lua
- Haskell
# decisions
- methods
	- V/Go-like `fn (t Type) method() ret { ... }`
	- Rust-like `trait`s `impl { ... }`
- assignment
	- V/Go-like `:=`
	- ~~Rust-like `let`~~
- error handling
	- Zig-like
	- V-like `or { ... }`
- kind of metaprogramming
- UCFS? `nim`
- modules / imports
- comments
	- `//`, `/* ... */` (nests supported)
	- `#`, `#[ ... #]` (nests supported)
- FFI
## targets
- V
- C
- Zig
- LLVM
- WASM
- μC
## implementation
- V
- Zig