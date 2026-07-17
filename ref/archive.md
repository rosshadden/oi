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
- ~~otter~~
# TODO
- [x] traits / interfaces
- [x] generics
- [x] methods
	- Rust-like `trait`s `impl { ... }`
	- ~~V/Go-like receivers `fn (t Type) method() ret { ... }`
- [x] assignment
	- V/Go-like `:=`
	- ~~Rust-like `let`
- [x] error handling
	- V-like `or { ... }`
	- ~~Zig-like
- [x] comments
	- `#`, `## doc`, `#[ ... ]#` or `#{ ... }#` or similar (nests supported) `nim gdscript`
	- ~~`//`, `/// doc`, `/* ... */` (nests supported) `rust`
- strings
	- [x] multiline syntax
	- [x] interpolation syntax
		- `println!("{} {2} {1} {foo}", a, b, c)` `rust`
		- `"$a $b $c"` `v bash`
		- `"`
		- `"%s %s %s" % [ a, b, c ]` `go python gdscript`
	- [x] raw syntax
		- `r"tagged string"` `v rust python`
		- ~~backtick? `nushell`
- [x] annotation / attribute / pragma / directive syntax
	- `@ann @ann("param")` `python java`
		- `@app.route("/")` `python`
	- ~~`@[ann] @[ann: "param"]` `v`
	- ~~`#[ann] #[ann(param)] #[ann(param: "value")]` `rust`
		- ~~worth noting this actually is compatible with / similar to my comment syntax, dunno if that's good or bad
- [x] pipe syntax
- [x] a built-in cascade/chain operator (or `with`), as if methods had a `return self` `clojure moonscript dart smalltalk`
- [x] solve the closure dissonance
	- lambdas vs. anonymous fns
	- explicit vs implicit captures
## consider
- doing `$in` differently
	- the concept is cool and work well but it's still a magic var
	- maybe rename it?: `$ ~ in _ :in`
- ~~UCFS? `nim`
- ~~explicit procedure overloading `odin`
- ~~`using` statement `odin`
	- don't need the full thing (because oi has embedded structs) but the "can pass in as if it were the used type" part is nice
- ~~inverting traits so they are explicit by default~~
### on multiple returns
Multiple returns are weird as they are, with lots of rough edges.
1. They could stay how they are, as sugary tuples.
	- oi would need to include a way to translate tuples <-> multiple values, like `table.un/pack` in `lua`
2. They could drop the parens `fn foo() int, bool { ... }`
	- then if you do have parens, it's just a tuple `fn foo() (int, bool) { ... }`
3. They could just be removed. I think having tuples and pattern matching / destructuring is enough.
	- I'm worried they might be a little plague-y, like `async`.
	- Can add back later if compelled.
# compiler %% fold %%
## possible compiler language
- Rust
- V
- Zig
## possible compilation targets
- V
- C
- Zig
- LLVM
- WASM
- μC