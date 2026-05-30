# facets
- trailing struct literals `v`
- strongly & statically typed `rust v go zig`
- everything is an expression `rust revo`
- destructuring / pattern matching `rust haskell`
- implicit
	- `return` `rust`
	- `result` `nim`
		- should maybe rename because `Result` is going to be a thing
		- `result`, `value`, `out`, `return`
	- `self` `lua revo`
		- I _think_ this is fully covered by the struct method syntax in `v` and `go`
	- `Self` `rust`
- opt-in mutable `rust v`
- pipes `Nushell revo`
- `:atoms`/keywoards `clojure janet elixir revo`
- optional types (`?`) `v zig`
- result types (`!`) `v`
- structs `c v go rust`
- comptime `zig revo nim`
- no parens needed for most expressions `v rust go`
- generics `rust v`
- metaprogramming
- block expressions `rust`
- everything is an expression `rust julia revo`
- compound types `rust`
- `discard`/`pass` `nim gdscript`
- macros `rust revo`
- multiple returns `v lua`
- enum literals `zig v`
- doc comments `zig v rust`
- namespaces `clojure revo`
- first-class
	- `assert` `rust v zig`
	- `test` (and also `suite` and `test/skip`) `zig revo`
	- `build` `zig revo`
	- units and unit conversion
- cli
	- `fmt` `v go`
	- `doc` `rust v go gdscript`
	- `test` `rust v go`
	- `lsp`
# syntax
- `fn main() { ... }`
- `const`
- free-form / C-like / whitespace insignificant
- `mut`
- `:=`
- no semicolons
- `.enum_literal`
- `loop`
## x/y
```rust
# Single line comments
# (can be stacked)

#[ Block comments
	#[ (can be nested) ]#
]#

## Doc comments.
##
## # support markdown
## ```
## # code block language defaults to itself
## ```

# functions

# implicit return
fn add(x int, y int) int {
	x + y
}

# implicit result
# (should maybe rename, because `Result` is going to be a thing)
fn random_user() User {
	result.name = "I Dunno"
}

# multiple returns
fn foo() (int, int) {
	2, 3
}

# structs

struct User {
	age int
	name string
	pos int = -1
}

# main entrypoint

fn main() {
	# variables
	no_mute := "immutable"
	mute := "mutable"
	mute = "trololololol"
	
	# unpack returns
	a, b := foo()

	# interpolation
	who := "mom"
	print("hi {who}!")
}
```
## playground
```rust
# struct method
fn (user User) can_register() bool {
	user.age > 16
}
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
- Odin
- Haskell
- GDScript
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
- metaprogramming how?
- UCFS? `nim`
- modules / imports
- comments
	- `#`, `## doc`, `#[ ... ]#` (nests supported) `nim gdscript`
	- ~~`//`, `/// doc`, `/* ... */` (nests supported) `rust`
- strings
	- multiline how?
	- interpolation how?
		- `println!("{} {2} {1} {foo}", a, b, c)` `rust`
		- `"$a $b $c"` `v bash`
		- `"`
		- `"%s %s %s" % [ a, b, c ]` `go python gdscript`
- printing
	- `println!()`
	- `println()`
	- `print()`
	- `echo()`
	- `puts()`
- FFI
- zeroed values? `v go`
- async how?
# vet
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
# name
## shortlist
- vex
- oi / o7
- sys
- ion
- kiln
- wire
- ~~ice
## misc
- noll
- loom
- rime
- Angela Lansbury
- polly
- lumen
- wyrm
- alloy
- nova
- eon / aeon
- egon
## animals
- axolotl
- ~~koi
- dog
- ~~otter
