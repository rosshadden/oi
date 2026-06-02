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
	- context `odin`
- opt-in mutable `rust v`
- pipes `Nushell revo`
	- error-specific pipes (revo uses `|>~` though I won't) `revo`
- `:atoms`/keywoards `clojure janet elixir revo`
- optional types (`?`) `v zig`
- result types (`!`) `v`
- structs `c v go rust`
- comptime `zig revo nim`
- no parens needed for simple conditionals `v rust go nu`
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
- operator overloading `lua nim v`
- `unreachable` `rust zig`
- `defer` `v go zig`
- `defer/err` (`errdefer`) `zig`
- zeroed values `v go`
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
- free-form / C-like / newline-sensitive, indent insensitive
- `mut`
- `:=`
- no semicolons
- `.enum_literal`
- `loop`
## playground
```rust
# Single line comments
# (can be stacked)

#{ Block comments
	#{ (can be nested) }
}#

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
	# this returns `result` because `(a.b = "c") == a`, like in revo
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

# struct method
impl User {
	fn can_register() bool {
		user.age > 16
	}
}

# main entrypoint

fn main() {
	# variables
	no_mute := "immutable"
	mut mute := "mutable"
	mute = "trololololol"
	
	# all types have zeroed values
	u := User{}
	assert(u.age == 0)
	assert(u.name == "")
	
	# unpack returns
	a, b := foo()

	# interpolation
	who := "mom"
	print("hi {who}!")

	# loops
	
	# forever
	loop {
	  print("are we there yet?")
	}
	
	# while
	mut i := 0
	loop i <= 3 {
	  print("are we there yet?")
		i += 1
	}
	
	# for
	loop i in 0..5 {
	  print(i)
	}
	
	# foreach
	loop x in [2 4 6 8] {
	  print(x)
	}
	
	# everything is an expression
	
	# returns the LHS object (`user`)
	user.age = 30
	assert((user.age = 30) == user)
	# this lets you do fun things like method chains on field setting:
	(user.age = 30).save()

	# ternary (`if` is an expression)
	foo := if true { "yes" } else { "no" }
	
	# Option and Result types

	nope := ?int(none)
	
	struct Repo {
		users []User
		some_optional_field ?int
		some_result_field !int
	}
	
	impl Repo {
		# result
		find_user(id int) !User {
			for user in self.users {
				if user.id == id { return user }
			}
			return error("User {id} not found")
		}

		# option
		find_user_if_exists(id int) ?User {
			for user in self.users {
				if user.id == id { return user }
			}
			return none
		}
	}
	
	user := repo.find_user(7) or { return }
	
	# errors
	
	# built-in Error trait
	trait Error {
		fn message(self) string
		fn code(self) int
		fn cause(self) ?Error { none }
	}
	
	# `!T` means: `T` or some value implementing `Error`
	fn read_config() !Config { ... }
	
	# crash out
	if false {
		assert(true, "optional message")
		panic("uh oh...")
	}
	
	# closures

	# lambda form
	nums.map(|x| x * 2)
	sort_by(|a, b| a.age < b.age)
	xs.filter(|x| x > 0)
	
	# anonymous functions
	adder := fn (n int) int { n + n }
	spawn(fn () { do_work() })
	
	# trailing closures
	
	# test("registration", { 
	#	 user := make_user()
	#	 assert(user.can_register()) 
	# })
	test "registration" {
		user := make_user()
		assert(user.can_register())
	}
	
	# db.transaction({|tx| ...})
	db.transaction {|tx|
		tx.insert(user)
		tx.insert(order)
	}
	
	# mutex.with({ do_work() })
	mutex.with {
		do_work()
	}

	# metaprogramming
	
	# compile-time eval with comp
	
	# takes any expression
	const PI = comp 22.0 / 7.0
	const VERSION = comp git.current_sha()
	
	# including if and match expressions
	const PLATFORM_DEFAULT = comp if BUILD_OS == :windows { "\\r\\n" } else { "\\n" }
	
	# embedded resources
	const image = comp fs.read_bytes("assets/cats.png")!
	const shader = comp fs.read("shaders/urmom.glsl")!
	
	# or block expressions
	const VERSION_INFO = comp {
		sha := git.head_sha()
		branch := git.current_branch()
		"{branch}@{sha[0..7]}"
	}
	const CONFIG = comp {
		raw := fs.read("build.toml")!
		toml.parse(raw)!
	}
	comp {
		# comptime assertions
		assert(max_connections > 0 && max_connections <= 65535)
	}
	
	# function calls can have comptime args
	fn open_typed(comp T type, path string) !T {
		raw := open(path)!
		deserialize<T>(raw)
	}
	
	# generics are sugar for comp type params
	fn first<T>(xs []T) ?T {
		if xs.len() == 0 { none } else { Some(xs[0]) }
	}
	# generics can have trait guards
	fn max<T: Ord>(a T, b T) T {
		if a > b { a } else { b }
	}
	fn max(comp T type, a T, b T) T where T: Ord { ... }
	
	# macro functions run at comptime and operate on ASTs
	macro derive_debug!(input Ast) Ast {
		# input is the parsed struct
		# build and return an `impl Debug for ...` block
		let fields = input.struct_fields()
		quote {
			impl Debug for $(input.name) {
				fn debug(self) string {
					# ... build using $fields
				}
			}
		}
	}
	macro derive_eq!(input Ast) Ast {
		let name = input.type_name()
		let fields = input.struct_fields()
		quote {
			impl Eq for $name {
				fn eq(self, other Self) bool {
					$(fields.map(|f| quote { self.$f == other.$f }).join(" && "))
				}
			}
		}
	}
	
	# macro calls end in a !
	# can be used for decorators
	
	@derive_eq!
	struct Point { x int, y int }

	@derive_debug!
	struct User { name string, age int }
	
	# and for inline calls
	vec!(1, 2, 3)
	
	# reflection in `comp`
	fn debug_print<T>(value T) {
		comp for field in type_info(T).fields {
			println("{field.name} = {value.(field.name)}")
		}
	}
	
	# conditional compilation
	fn log(msg string) {
		comp if BUILD_MODE == :debug {
			eprintln(msg)
		}
	}
}

# stdlib

# this is stdlib print
fn print<T: Display>(value T)

print(value) # stdout, with newline
write(value) # stdout, no newline
eprint(value) # stderr, with newline
ewrite(value) # stderr, no newline
macro dbg!(expr) # debug-print, value passthru
```
## lab 
Things I'm playing with that might not work or make it.
```rust
"foo"
	|> upper()
	-> upper()
	| upper()

"error-only pipes"
	|>~ upper()
	~> upper()
	|e upper()
	
# if any step returns none, the whole chain is none
"optional-aware"
	|>? upper()
	?> upper()
	->? upper()
	?| upper()
	-?> upper()
	|? upper()
	|?> upper()

# any error short circuits
"result-aware"
	|>! upper()
	!> upper()
	!-> upper()
	!| upper()
	|!> upper()
	-!> upper()
	|! upper()
	
result := data
    |> validate
    |> transform(opts)
    |> save
# desugars to: save(transform(validate(data), opts))

formatted := name
    |> uppercase
    |> wrap("[", _, "]")     # _ marks where the threaded value goes
    |> log(level: :info, _)
```
# influences
- V
- [revo](https://github.com/if-not-nil/revo)
- Nushell
	- structured data pipelines
- Rust
	- `impl`
- Nim
- Zig
- Clojure
	- metaprogramming
	- homoiconicity
- Janet
- Lua
- Odin
- Julia
- Elixir
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
	- `#`, `## doc`, `#[ ... ]#` or `#{ ... }#` or similar (nests supported) `nim gdscript`
	- ~~`//`, `/// doc`, `/* ... */` (nests supported) `rust`
- strings
	- multiline syntax?
	- interpolation syntax?
		- `println!("{} {2} {1} {foo}", a, b, c)` `rust`
		- `"$a $b $c"` `v bash`
		- `"`
		- `"%s %s %s" % [ a, b, c ]` `go python gdscript`
	- raw syntax?
		- backtick? `nushell`
- printing
	- `println!()`
	- `println()`
	- `print()` and `print_raw()` or something
	- `echo()`
	- `puts()`
- FFI
- async how?
- some sort of `todo`/`unimplemented` (but I'd rather keep them out of the global namespace) `rust`
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
