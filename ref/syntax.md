# [[Oi|../]]

```rust
## comments

# Single line comments
# (can be stacked)

#{ Block comments
	#{ (can be nested) }#
}#

## Doc comments.
##
## # support markdown
## ```
## # code block language defaults to Oi
## ```

## modules

#{
	For now Oi is going to do what V does for modules.
	directory == module
	I don't love how it handles nested modules, but I will likely revisit this in the future.
}#

# specify module
module module_name

# imports
import os

# selective imports
import os { input }

# import aliases
import crypto.sha256
import mymod.sha256 as mysha256

## functions

# private within module by default
fn foo() {
	print("foo")
}

# use `pub` modifier to make visible to outside modules
pub fn bar() {
	print("bar")
}

# implicit return

fn add(x int, y int) int {
	x + y
}

fn random_user() User {
	user := User{}
	user.name = "I Dunno"
	user
}

# implicit input data

# `$` is the data passed to a function
# TODO: mutable `$` by default or opt-in?

# `$` directly matches the call signature, so it is strongly typed and enforceable by the compiler
fn single_val(x int) {
	assert!(x == $)
}
fn one_tuple(x int,) {
	assert!(x == $.0)
}
fn two_tuple(x int, y int) {
	assert!(x == $.0)
	assert!(y == $.1)
}

# named returns

## Bound name is initialized as a mutable zero-value of the specified return type.
## I feel like it's ergonomic and not too magical, but maybe others will disagree.
##
## Although Oi looks like V and in turn Go, and Go has a named return feature itself (which V interestingly did not copy),
## I really attribute this more to Nim's implicit `result`:
## - https://nim-by-example.github.io/variables/result/
## - 
## Nim's `result` is great, but I don't like that it's magic.
## So I opted to make you opt-in by naming it explicitly.
## It's less ergonomic this way and I might just go back to implicitly initialized `result` or `$out` var like I had originally.

# building on what Go does, a bare `return` updates the bound values
fn two() result int {
	assert!(result == 0)
	result = 2
	return
}
assert!(two() == 2)

fn random_user() u User {
	print(u) # User{}
	u.name = "I Dunno"
}
ru := random_user()
assert!(ru.name == "I Dunno")

# this really just skips the step of explicitly initializing a zeroed var
fn divmod(a int, b int) out (int, int) {
	out.0 = a / b
	out.1 = a % b
	return
}

## pure functions

# `@pure` is a compiler-verified contract for deterministic functions with no side effects.
# A @pure fn may not perform IO, read/write globals or module-level state, or call non-pure functions.
# @pure implies non-capturing (no enclosing locals), so [] is redundant alongside it.
# Can be applied to both named and anonymous functions.

@pure
fn add(x int, y int) int { x + y }

@pure
fn clamp(value f64, low f64, high f64) f64 {
	match true {
		value < low { low }
		value > high { high }
		else { value }
	}
}

# @pure fns may call other @pure fns
@pure
fn sum_of_squares(a int, b int) int {
	add(square(a), square(b))
}

# anon @pure
# useful when passing to a higher-order fn that requires a purity guarantee
# (or for theoretically possible optimization from the theoreticl compiler)
result := data.map(@pure fn (x int) int { x * x })

## leading literals

# if there's only one literal arg, the parens may be dropped
foo "bar"
print "lol"
sleep 1_000
log.group :process

# this can be used in conjunction with trailing functions
test "foo" { assert!(foo == bar) }
benchmark 1_000_000 { do_work() }
config :production { ... }
hook .startup { ... }

## structs

struct Point {
	x int
	y int
}

point := Point{
	x: 19
	y: 90
}

# one line
point := Point{ x: 19, y: 88 }

# zero values when unspecified
origin := Point{}

# support default field values
struct User {
	age int
	name string
	swag int = 5
}

# heap structs
# structs are allocated on the stack, but can be allocated on the heap with the `&` prefix
# this returns a reference
u := &User{}

# required fields
struct Foo {
	n int @required
}

# short struct literals
normal := Point{
	x: 2
	y: 1
}
short := Point{3, 2}

# struct update

struct User {
	name string
	age int
	is_registered bool
}

fn register(u User) User {
	return User{
		...u
		is_registered: true
	}
}

mut user := User{
	name: "abc"
	age: 23
}
user = register(user)

# trailing struct literals

struct Options {
	foo int
	bar bool
}
impl User {
	fn with_options(self, opt Options) {
		print(opt)
	}
}
user := User{}
user.with_options(bar: true, foo: 4)

# annotating with `@params` lets a trailing struct be omitted
# otherwise you need to specify at least one field or the compiler will error
@params
struct Settings {
	idk int
}
impl User {
	fn with_settings(self, settings Settings) {
		print(settings)
	}
}
user.with_settings()

# access modifiers

struct User {
	# fields are private and immutable by default
	name string
	age int
	
	# `pub` and `mut` modifiers can be used per definition, just like in normal declarations
	a bool
	mut b bool
	pub c bool
	pub mut d bool

	# these modifiers also have block forms for when grouping is desired
	mut {
		status Status
		retries int
	}
	pub {
		email string
		phone string
	}
	pub mut {
		last_login Time
		session_id UUID
	}
}

# anonymous structs

struct Food {
	name string
	nutrition struct {
		calories int
	}
}

apple := Food{
	name: "apple"
	nutrition: struct {
		calories: 4
	}
}

# you can (maybe?) use short struct literals in the assignment
pear := Food{
	name: "pear"
	nutrition: struct { 5 }
}

# static struct methods
impl User {
	fn new() Self {
		Self {}
	}
}
user := User.new()

# struct methods
impl User {
	fn can_register(self) bool {
		self.age > 16
	}
	fn set_age(mut self, age int) {
		self.age = age
	}
}

# embedded structs

struct Profile {
	Options
	name string
}

profile := Profile{
	foo: 4
	name: "one cool dude"
}
assert! profile.foo == profile.Options.foo

# you can refer to and assign to embedded structs directly
profile := Profile{
	Options: Options{
		foo: 200
	}
}
print(profile.Options)
profile.Options = Options{}

# operator overloading

struct Point {
	x int
	y int
}
impl Add for Point {
	fn add(self, other: Self) Self {
		Self{self.x + other.x, self.y + other.y}
	}
}
assert!(Point{1, 0} + Point{2, 3} == Point{3, 3})

## traits

# a trait is a set of behaviors and/or data
trait Animal {
	# field requirement
	kind string

	# method requirement
	fn speak(self) string

	# default methods build on the requirements
	# may be overridden
	fn shout(self) string {
		self.speak().upper()
	}
}

struct Dog { kind string }
struct Cat { kind string }

# traits are satisfied by an explicit `impl`
impl Animal for Dog { fn speak(self) string { "woof" } }
impl Animal for Cat { fn speak(self) string { "meow" } }

# an embedded struct can satisfy requirements too
struct Meta { kind string, id int }
struct Enemy {
	Meta
	hp int
}
impl Animal for Enemy { fn speak(self) string { "rawr" } }

fn demo_traits() {
	dog := Dog{"Collie"}
	cat := Cat{"Egyptian Mau"}
	animals := []Animal{ dog, cat }
	
	loop animal in animals {
		print "a {animal.kind} says: {animal.speak()}"
	}
}

# implementing traits typically requires explicit `impl` blocks
struct Person {
	kind string = "Human"
}
impl Animal for Person {
	fn speak(self) string { "Lorem ipsum..." }
}
# TODO: nail down syntax for trait checking. this is english for clarity, not final form
assert!(Person is of Animal)

# traits can use `@implicit` to opt-in to structural / duck typing
# any type with the right shape satisfies it even without an impl block
@implicit
trait Fruit {
	seeds bool
	color Color
}
struct Kiwi {
	seeds bool = true
	color Color = :green
}
struct Apple {
	seeds bool = true
	color Color = :red
}
struct Bike {
	color Color = :purple
}
impl Fruit for Apple
assert!(Kiwi is of Fruit)
assert!(Apple is of Fruit)
assert!(Bike is not of Fruit)

## static vs dynamic dispatch

# A trait used as a bound is static: monomorphized per concrete type.
# no vtable, no indirection, no allocation
fn greet<A: Animal>(a A) { print(a.shout()) }

# A trait used directly as a type is dynamic: a trait object behind a vtable.
zoo := []Animal{ Dog{"collie"}, Cat{"mau"}, Enemy{ kind: "boss", hp: 9 } }
loop a in zoo { print "a {a.kind} says {a.speak()}" }

## associated types

# a related type chosen per impl
trait Iterator {
	type Item
	fn next(mut self) ?Item
}
struct Range { mut cur int, end int }
impl Iterator for Range {
	type Item = int
	fn next(mut self) ?int {
		if self.cur >= self.end { return none }
		defer self.cur += 1
		self.cur
	}
}

## supertraits

# require another trait alongside this one
# every Ord is also an Eq
trait Ord: Eq {
	fn cmp(self, other Self) Ordering
}
fn max<T: Ord>(a T, b T) T {
	if a.cmp(b) == .greater { a } else { b }
}

## associated constants

trait Bounded {
	const min Self
	const max Self
}
impl Bounded for i8 {
	const min = -128
	const max = 127
}

## blanket impls

# implement a trait for every type that already meets a bound
trait ToString {
	fn to_string(self) string
}
impl<T: Display> ToString for T {
	fn to_string(self) string { self.display() }
}

## marker traits

# traits don't have to have methods or fields or anything
trait Copy {}
impl Copy for Point {}

## composite types

#{
	every type constructor composes with every other, to any depth
	
	| Oi Syntax | Meaning | Rust Equivalent |
	| --- | --- | --- |
	| `[]T` | Dynamic array | `Vec<T>` |
	| `[N]T` | Fixed array | `[T; N]` |
	| `map[K]V` | Map | `HashMap<K, V>` |
	| `(A, B)` | Tuple | `(A, B)` |
	| `?T` | Optional | `Option<T>` |
	| `!T` | Result | `Result<T, _>` (error is any `Error`) |
	| `&T` | Reference / heap | `&T` |
	| `fn (A) R` | Function | `fn(A) -> R` |
	| `Foo<T>` | Generic instance | `Foo<T>` |
	
	the prefix constructors (`[]` `[N]` `map[K]` `?` `!` `&`) nest ltr
	leftmost is outermost, the same way you read Rust's A<B<C>> from the outside in
}#

# Rust: Result<Vec<Option<String>>, io.Error>
type Parsed = ![]?string

# order matters
# these are different types:
| --- | --- | --- |
| ?[]int | Option<Vec<i32>> | the whole list may be absent
| []?int | Vec<Option<i32>> | each slot may be absent

# nest as deep as you like, no special grouping needed
struct Stress {
	groups map[string][]User
	cache map[string]?[]u8
	handlers []fn (Request) !Response
	matrix [4][4]f32
	maybe ?&User
	tree Tree<![]?string>
}

# ?T and !T are the ergonomic default. drop to the explicit enum forms (see enums)
# when you need to name or constrain the wrapped / error type:
# - `?T  ==  Option<T>`
# - `!T  ==  Result<T, E>`   for some E: Error
fn read(path string) Result<[]u8, io.Error> { ... } # error pinned
fn slurp(path string) ![]u8 { ... } # error left open

# user generics nest exactly like the built-ins, they aren't special
type Grid<T> = [][]T
type Lookup<V> = map[string]?V

## main entrypoint

fn main() {
	## variables
	
	# assignment
	
	# declaration without assignment
	mut foo int # 0
	mut bar string # ""
	mut p Point # Point{}
	mut grid [3]string # ["", "", ""]
	
	# declaration with assignment
	a int := 2
	b string := "hi"
	c Car := Car{}

	# inferred
	no_mute := "immutable"
	mut mute := "mutable"
	mute = "trololololol"
	
	# muliple assignment
	(foo, bar) := ("food", "bard")
	(lat long) := get_coords()
	
	# swap
	(mut baz, mut qux) := ("bazd", "quxd")
	(baz, qux) = (qux, baz)
	
	## primatives
	
	bull := true
	str := "string"
	integer := 1337
	flt := 69.420
	
	# ranges
	# TODO: are until/after possible outside array slices?
	between := 1..3
	until := ..3
	after := 1..
	crossing_over_with_john_edward := -4..4
	
	# paths
	# TODO: path literal
	
	# numbers
	
	# number litarals are `int` (`i32`) and `float` (`f64`) unless otherwise indicated
	i := 55 # int AKA i32
	f := 55.55 # float AKA f64
	e_notation_float := 10e2 # 1000.0
	
	# can use a prefix to denote common notations
	# these are all 123
	a0 := 123
	a1 := 0x7B
	a2 := 0b01111011
	a3 := 0o173
	
	# can separate arbitrarily with `_`
	bil := 1_000_000_000
	wtf := 1_2_3_4_5
	floater := 10_000.22
	binary_mask := 0b1_1111_1111
	permissions := 0o7_5_5
	big_addr := 0xFF80_0000_0000_0000
	
	# can cast between types
	big_int := i64(50_000)
	small_unsigned_int := u8(16)
	
	# ints can be automatically promoted to f64 or larger-width ints
	assert!(2 + 1.0 == 3.0)
	
	# supports arbitrary bit-width integers, like Zig
	# use `i<width>` and `u<width>`, where width in [1, 65535]
	weird_one := i2(1)
	wat := u7(1000)
	
	# supported floating types are: f16 f32 f64 f80 f128
	
	# strings
	
	normal := "NORMAL mode"
	raw := r"there is no\nescape"
	regex := r"\d+\.\d+"
	multiline := "
		strings are multiline
		by default
	"
	
	# concatenation
	assert!("foo" + "bar", "foobar")
	
	# string interpolation
	who := "mom"
	print("hi {who}!")

	# any expression works inside braces
	user := User { name: "alice", age: 30 }
	print("{user.name} is {user.age}")
	print("sum: {2 + 2}")
	print("upper: {who.uppercase()}")
	
	# escape braces by doubling
	print("use {{braces}} like this")
	
	# works in multiline strings
	msg := "
		dear {who},
		your balance is {amount}.
	"
	# but no interpolation in raw strings
	path := r"C:\Users\{who}" # {who} is not interpolated
	
	# arrays

	# collection of 0-indexed elements of the same type
	names := ["john", "jacob", "jingleheimerschmidt"]
	print(names)
	# can be accessed with an index expression
	assert!(names[1] == "jacob")
	i := 1
	assert!(names[i] == names[1])
	# numbers literals may also be used with dot notation
	assert!(names.0 == "john")
	assert!(names.2 == "jingleheimerschmidt")
	
	# append with `<<`
	mut odd := [1, 3, 5]
	odd << 7
	assert!(odd.3 == 7)
	# entire arrays can be appended too
	odd << [9 11]
	assert!(odd.5 == 11)
	assert!(odd.len == 6)
	
	# arrays support dropping the commas when only literals are present
	even := [2 4 6]
	
	# `in` operator returns whether array contains element
	assert!(6 in even)
	
	# arrays have fields
	# `len` is the number of initialized elements in the array
	assert!(even.len == 3)
	
	# array init
	mut arr := []int{}
	arr << 3
	
	# fixed size arrays
	mut three := [3]string{}
	three.0 = "larry"
	three.1 = "curly"
	three.2 = "moe"
	
	# maps
	
	num_map := {
		one: 1
		two: 2
	}
	print(num_map["one"])
	mut typed_map := map[string]int{}
	typed_map["three"] = 4
	typed_map.delete["three"]
	
	# array slices are array subsets of another array
	# proper array
	even := [0 2 4 6 8]
	# slices of it
	assert! even[1..3] == [2 4]
	assert! even[..3] == [0 2 4]
	assert! even[1..] == [2 4 6 8]
	
	# tuples
	
	# tuples are very important in Oi
	# under the hood many things are tuples, and some if it bleeds through in [hopefully] interesting ways
	# function input params are [planned to be] treated as tuples in the compiler
	
	# the `$` var you've seen in other places makes this really clear
	fn its_all_tuples_man(a bool, b int, c string) (bool, int, string) {
		$
	}
	result := its_all_tuples_man(true, 2, "lol")
	print(result) # (true, 2, "lol")
	
	# tuples support dropping the commas when only literals are present
	only_nums := (2 3 4)
	other_literals := ("lisp, innit?" true [2 4 5])
	
	# named tuple fields
	
	## Naturally every tuple field has a positional index.
	## But they can also optionally be given names.
	## This should remind the reader of tables in Lua (and Revo <3).
	
	t := (a: 1, b: 2)
	print(t) # (a: 1, b: 2)
	assert!(t.a == t.0)
	assert!(t.b == t.1)
	
	#{
		These names are purely aliases / hints, and do _not_ affect identity or comparison.
		Think of it like somebody asks us if their rock is the same as our rock.
		We can tell that they are the same, we just happen to know a lot more details about our rock than theirs.
		I've never been great with analogies.
		Anyway don't abuse this. The field names are for convenience, not as a replacement for structs.
	}#
	assert!((x: 4, y: 2) == (4, 2))
	assert!((x: 4, y: 2) == (4, z: 2))
	
	# names do not need to be given to all indices
	t := (1, b: 2)
	print(t) # (1, b: 2)
	assert!(t.b == t.1)
	
	# can be used in function return signatures
	fn split(value string) (left string, right string) {
		split_once(value, "|") # returns a 2-tuple (a twople? anyone?)
	}
	splat := split("hi|mom")
	(l, r) := split("hi|mom")
	assert!(splat.left == "hi")
	assert!(splat.right == "mom")
	assert!(splat == (l, r))
	
	# another example with a common divmod method
	fn divmod(a int, b int) (q int, r int) {
		(a / b, a % b)
	}
	result := divmod(10, 3)
	print(result) # (q: 3, r: 1)
	assert!(result == (3, 1))
	assert!(result.0 == 3)
	assert!(result.1 == 1)
	assert!(result.q == 3)
	assert!(result.r == 1)

	# this can be used alongside the named return feature, as they are different systems
	fn divmod(a int, b int) out (q int, r int) {
		out.q = a / b
		out.r = a % b
		return
	}
	
	fn http_get(url string) (int, body string, []Header) {
		(200, "the body", [])
	}
	result := http_get("/health")
	print(result) # (200, body: "the body", [])
	assert!(result.body == result.1)
	
	## unit type
	
	# (), a 0ple, is the unit type
	# when you have a fn with no return type expressed, it returns `()`
	assert!(() == ())
	
	# these are all equivalent:
	fn nada() {}
	fn zilch() () {}
	fn nope() {
		()
	}
	fn no_way() {
		return ()
	}
	fn nuh_uh() {
		return
	}
	assert!(nada() == zilch())
	assert!(nada() == nope())
	assert!(nada() == no_way())
	assert!(nada() == nuh_uh())
	assert!(nada() == ())
	
	## never
	
	# `never` indicates that a fn should not return
	fn foo() never {
		loop {}
	}
	foo()
	unreachable!("the above fn should never have finished")
	
	## atoms
	
	# Oi has first-class atoms
	:foo
	assert!(:foo != :bar)
	food := :apple
	assert!(food == :apple)
	
	# atoms coerce to enum variants when the type is known from context
	# NOTE: atoms by definition cannot carry payloads
	enum Color { red blue }
	mut c := Color.red # fully qualified
	c = .red # type inferred from declaration
	c = :blue # type inferred from declaration and coerced
	assert!(c == Color.blue)
	assert!(Color.blue == :blue)
	
	enum Stat { health mana stamina }
	struct User {
		mut stat Stat
	}
	user1 := User{ stat: .mana }
	user2 := User{ stat: :mana }
	assert!(user1.stat == user2.stat)

	# this might be useful for quick prototyping?
	# nothing at the callsites needs to change when you later add the definition
	# NOTE: TBH I might remove this feature or make it a compiler warning when a typed enum exists.
	
	# prototype code
	mut state := :loading
	state = :ready
	
	# on a later pass, despite nothing at the callsites changing, adding this enum definition would add strong typing and copiler checking
	# STYLE: if an enum exists, prefer `.foo`
	enum State { loading ready error }
	
	## types
	
	# type aliases
	type Score = int
	type Speed = (Point, int)
	
	# function signatures can be aliased
	type Operation = fn (int) int
	fn op(n int, f Operation) int {
		return f(n)
	}
	fn double(n int) int {
		return 2 * n
	}
	# explicit cast
	print(op(4, Operation(double))) # 8
	# duck typing
	print(op(4, double)) # 8
	# anonymous function
	print(op(4, fn (n int) int {
		return 3 * n
	})) # 12
	# anonymous function shorthand (types inferred, input accessible via `$`)
	print(op(4, fn { $ * 4 })) # 16
	
	# all types have zeroed values
	u := User{}
	assert!(u.age == 0)
	assert!(u.name == "")
	
	## control flow
	
	i := 2
	if i == 0 {
		print("zero")
	} else if i == 1 {
		print("one")
	} else {
		print("idk")
	}
	
	## matching
	
	# else for catch-all
	os := "linux"
	match os {
		"darwin" { print("I used to hate macOS but now I realize it's at least better than Windows.") }
		"linux" { print("I use Artix Linux btw") }
		else { print(os) }
	}

	# can be used as an if-else chain
	# evaluated in order, first match wins if multiple satisfy the condition

	# comma can be used to test multiple values
	fn is_red_or_blue(c Color) bool {
		return match c {
			.red, .blue { true }
			.green { false }
		}
	}

	# TODO: not sure whether Oi should support `$` in match or use binding
	match user {
		u @ User { age: 0..18 } => "minor: {u.name}"
		User { age: 0..18 } => "minor: {$.name}"
		_ => "adult"
	}

	## loops
	
	# `loop {}`: infinite
	# `loop <cond> {}`: while
	# `loop <pattern> in <iter> {}`: for
	
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
	loop (x, y) in [(0, 0) (1, 2)] {
	  print((y, x))
	}
	
	# TODO: custom iterators
	
	## [almost?] everything is an expression
	
	# ternary (`if` is an expression)
	foo := if true { "yes" } else { "no" }
	
	# if no else, uses default value from the if body
	# TODO: or should it be `none` and make the var `?T`?
	str := if false { "idk" }
	num := if false { 42 }
	assert!(str == "")
	assert!(num == 0)

	# built-in functions
	result := assert!(check()) |> next
	
	# match
	(i, foo, bar, u, me) := (0, true, true, 2, [0 2 4])
	n := match true {
		i < 3 { "love ya" }
		foo == bar { "soul mates" }
		u in me { "🥵" }
		else { "no dice" }
	}

	## `Option` and `Result` types
	
	# `?T` holds `some(T)` or `none`
	# `!T` holds `ok(T)` or an `error` (any type implementing the `Error` trait)
	# bare return values are auto-wrapped
	# there is no need for an explicit `ok()` or `some()` un/wrapper like there is in Rust
	
	struct Repo {
		users []User
		cached_name ?string # zero value is `none`
	}
	
	impl Repo {
		# !T returns a value or an error
		fn find_user(id int) !User {
			loop user in self.users {
				if user.id == id { return user }
			}
			return error("User {id} not found")
		}
	
		# ?T returns a value or `none`
		fn find_user_if_exists(id int) ?User {
			loop user in self.users {
				if user.id == id { return user }
			}
			return none
		}
	}
	
	# ?T and !T must be handled, and the or block is required to unwrap
	# $ is the Error value (!T) or none (?T)
	user := repo.find_user(7) or {
		print($.message())  # "User 7 not found"
		return
	}
	
	# or block can yield a fallback value of the same type
	user := repo.find_user(7) or { User{ name: "guest" } }
	
	# check error type in the or block
	file := fs.open(path) or {
		if $ is fs.NotFoundError { return create_default() }
		panic($.message())
	}
	
	# postfix `!` propagates error up to the caller
	# caller must return !T, or it panics if used in main()
	fn load_config(path string) !Config {
		raw := fs.read(path)!
		parse(raw)!
	}
	
	# postfix `?` propagates none up to the caller
	# caller must return ?T
	fn display_name(id int) ?string {
		user := repo.find_user_if_exists(id)?
		user.name
	}
	
	# creating option/result values directly
	nope   := ?int(none)
	maybe  := ?int(42)
	ok	 := !int(7)
	broken := !int(error("oops"))
	
	# ?T / !T wrap the whole tuple in multi-return
	fn checked_divmod(a int, b int) !(int, int) {
		if b == 0 { return error("division by zero") }
		(a / b, a % b)
	}
	(q, r) := checked_divmod(10, 3)!
	
	# custom error types
	# embed Error for default impls, only override what you need
	
	struct ParseError {
		Error
		line int
		col  int
	}
	impl ParseError {
		fn message(self) string { "parse error at {self.line}:{self.col}" }
		fn code(self) int { 1 }
	}
	
	fn parse(src string) !Ast {
		...
		return ParseError{ line: 4, col: 2 }  # auto-cast to Error
	}
	
	parse(src) or { panic($.message()) }
	
	# error chaining via cause()
	struct WrappedError {
		Error
		msg   string
		inner Error
	}
	impl WrappedError {
		fn message(self) string { self.msg }
		fn cause(self) ?Error  { self.inner }
	}

	
	## enums
	
	# plain
	enum Color {
		red
		green
		blue
	}
	# fully-qualified enum, for when inference can't help
	mut c := Color.green
	# shorthand enum when the type is known from context
	c = .red
	
	# variants with payloads
	enum Shape {
		circle { radius f64 }
		rectangle { width f64, height f64 }
		triangle(f64, f64, f64)
		point
	}
	s := Shape.circle { radius: 5.0 }
	s := .circle { radius: 5.0 }
	s := Shape.triangle(3.0, 4.0, 5.0)
	s := Shape.point
	
	# pattern matching (exhaustive)
	area := match s {
		.circle { radius } => PI * radius * radius
		.rectangle { width, height } => width * height
		.triangle(a, b, c) => heron(a, b, c)
		.point => 0.0
	}
	
	# specified values
	enum Status: int {
		ok = 200
		not_found = 404
		server_error = 500
	}
	
	# ?T and !T are syntax suger for these:
	enum Option<T> {
		some(T)
		none
	}
	enum Result<T, E> {
		ok(T)
		err(E)
	}
	
	# first value is default
	c := Color{} # .red
	s := Shape{} # .circle { radius: 0.0 }
			
	# methods
	
	enum Color {
		red
		green
		blue
	}
	
	impl Color {
		fn hex(self) string {
			match self {
				.red => "#ff0000",
				.green => "#00ff00",
				.blue => "#0000ff",
			}
		}
		
		fn is_warm(self) bool {
			self == .red
		}
		
		# Associated function (no self)
		fn primary() Color {
			.red
		}
	}
	
	# Display is auto-derived for enums, but can be overridden
	impl Display for Color {
		fn display(self) string {
			match self {
				.red => "🔴",
				.green => "🟢",
				.blue => "🔵",
			}
		}
	}
	
	c := Color.red
	print(c.hex()) # "#ff0000"
	default := Color.primary()
	
	# enums can be created from string or integer value and converted into string
	
	enum Cycle {
		one
		two = 2
		three
	}
	
	// create enum from value
	print(Cycle.from(10) or { Cycle.three })
	print(Cycle.from("two")!)
	
	// convert an enum value to a string
	print(Cycle.one.str())
	
	# the newlines are optional
	enum Fruit { apple orange grape }

	## errors
	
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
		assert!(true, "optional message")
		panic("uh oh...")
	}
	
	## blocks
	
	# blocks are groups of expressions
	# the final expression is the block's value
	three := {
		light_the_beacons()
		3
	}
	
	# `;` joins lines
	long_but_short := { do_thing(); 3 }
	
	# blocks are eager and run in place
	# they can fully read and mutate the enclosing scope
	
	## anonymous functions
	
	#{
		Anonymous functions may be created with `fn`.
		The syntax scales from tiny closures to fully typed, explicitly captured functions.
		All parts behave just as they do in named functions (except captures, which are unique to anon fns).
		```
		# NOTE: `ret` takes precedence over `name`: if only one identifier is present Oi treats it as the return spec, which is far more common than naming an anon fn.
		fn name? [captures]? (params)? ret? { body }
		```
		- name: optional name declaration
		- captures: optional capture spec
			- omitted: fn implicitly captures any enclosing locals it references, read-only by reference
			- []: non-capturing. holds no closure environment, so it coerces to a plain function-pointer type and can be stored freely.
			  can still call named functions and read module-level consts/types
			- [x]: captures `x` read-only (by reference), and nothing else
			- [mut x, y]: captures `x` mutably, `y` read-only, and nothing else
			- [move x]: moves/owns `x` so it can escape the enclosing scope, and nothing else
			- any bracket (empty or populated) turns implicit capture off, and all referenced locals must then be listed explicitly
		- params: optional param spec
		- ret: optional return spec
	}#

	# implicit capture
	n := 10
	scale := fn (x int) int { x * n }

	# non-capturing
	# NOTE: This does not mean pure. See [pure functions](#pure-functions)).
	mul := fn [] (x int, y int) int { x * y }
	nums.map(fn [] { $ * 2 })

	# explicit read-only capture
	factor := 3
	triple := fn [factor] (x int) int { x * factor }

	# explicit mutable capture
	mut counter := 0
	increment := fn [mut counter] (x int) int {
		counter += x
		counter
	}
	
	# move capture
	spawn(fn [move data] { process(data) })
	
	## trailing functions
	
	# if a function is the last argument of a call, it may be written after the parens
	retry(3) fn {
		fetch(url)!
	}
	
	# if no named params are needed, the `fn` may be omitted (`$` is still accessible)
	retry(3) {
		fetch(url)!
	}
	
	# if the trailing function is the only argument, the parens may be omitted too
	spawn {
		do_work()
	}
	mutex.with { do_work() }
	
	# composed with leading literals, function calls may be written like this:
	# test("registration", fn { ... })
	test "registration" {
		user := make_user()
		assert!(user.can_register())
	}
	retry 3 {
		fetch(url)!
	}
	timeout 5.sec {
		slow_call()
	}
	
	# like with normal functions, `$` is the input passed to the anonymous function
	db.transaction {
		$.insert(user)
		$.insert(order)
	}
	
	# named (and typed) params may optionally be provided
	db.transaction fn (tx) {
		tx.insert(user)
		tx.insert(order)
	}
	
	## misc.
	
	# defer
	
	# defer takes an expression
	mut f := os.create("out.log")!
	defer f.close()
	
	# blocks are expressions too
	defer {
		print("closing file")
		f.close()
	}
	
	# defer gets the return values if relevant
	fn do_stuff() bool {
		defer {
			if !$ {
				print("uh oh...")
			}
		}
		if os.env("DEBUG") { return false }
		return true
	}

	# defer/err only runs if an error was raised
	defer/err eprint()
	
	# defers in loops run at the end of each iteration
	loop {
		defer print("here we go again...")
		do_stuff()
	}

	## pipelines

	input := "let's do this"
	result := input |> trim |> upper
		
	# if any step returns none, the whole chain is none
	"optional-aware" |> upper?
	nickname := find_user(id)
		|> get_profile?
		|> get_display_name?
		or "anonymous"
	
	# any error short circuits
	"result-aware" |> upper!
	result := input |> trim |> upper |> save!
	
	# Each step gets the piped value as `$`.
	# A bare fn (ex: `trim`) is ran with the input as the first param (`trim` == `trim($)`).
	# Any other expression (a call using `$`, an `if`, a block) # is evaluated in place with `$` bound.
	# This lets us do clojure-like threading.
	"threading"
	  |> wrap("[", $, "]")
	  or log_errors("foo", $)
	"hello" |> $ + " world"
	[2 4 6 8] |> if $.len() > 0 { print(true) }
	
	# any errors in the pipeline flow directly to an `or`
	"error-only pipes"
		|> upper
		or handler
	
	# any expression can be used as a pipeline step, including blocks
	# for convenience, in blocks `$` is bound to the passed-in params as if they were a function
	result := "error-only pipes with block"
		|> {
			idk($)
		}
		|> {
			log.info("stuff and things: {$}")
			:block_done
		}
		|> fn {
			assert!($ == true)
			log.info("this is an _actual_ function")
			:fn_done
		}
		or {
			eprint($)
			return $
		}
	assert!(result == :fn_done)
	config := os.env("config_path")
		|> read_file!
		|> parse!
		or {
			log.warn("Config load failed: {$}. Using default.")
			default_config()
		}
	"gtfo" |> process or { panic("uh oh...") }
	"err binding" |> raise_err |> or { log.error($) }
	
	# you can specify params
	#  to a name when nesting to avoid ambiguity
	"foo" |> fn (outer) {
		outer |> fn (inner) {
			log.debug("inner: {inner}, outer: {outer}")
		}
		assert!(outer == $)
	}

	# or you can cache the `$`
	"foo" |> {
		outer := $
		outer |> {
			inner := $
			log.debug("inner: {inner}, outer: {outer}")
		}
		assert!(outer == $)
	}
	
	# all together now (all together now!)
	result := data
		|> validate
		|> transform(4, $.name)?
		|> filter($ > 0)
		|> send?
		|> wrap("[", $, "]")
		|> {
			log.info("saving {$}...")
			save($)!
		}
		or log
	
	formatted := name
		|> uppercase
		|> wrap("[", $, "]")
		|> log(level: :info, $)
		
	# pipeline functions

	# there is a shorthand for creating methods that are just made up of a pipeline
	# the following function:
	fn slugify(s string) string {
		s |> trim |> lower |> replace(" ", "-")
	}
	# may be written like this:
	fn slugify = trim |> lower |> replace(" ", "-")
	# type annotations may be provided for clarity, but are inferred from the pipeline
	fn slugify(s string) string = trim |> lower |> replace(" ", "-")
	
	# this lets any bound values be used directly throughout the pipeline,
	# rather than each stage only having access to the return of the prior stage
	fn count_letters(s string) int =
		lower |> uniq |> replace("[^A-Za-z]", "") |> len |> {
			log.info("called count_letters with {s}, and it has {$} unique letters")
			$
		}
	assert!(count_letters("hi, mom!") == 4)
	
	## metaprogramming
	
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
		assert!(max_connections > 0 && max_connections <= 65535)
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
	
	# macros
	# macro calls end in a !
	
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
					$(fields.map(fn (f) { quote { self.$f == other.$f } }).join(" && "))
				}
			}
		}
	}
	
	# can be used for decorators
	
	# equivalent to `@derive(Equal)` with a common default handler
	@derive_eq!
	struct Point { x int, y int }
	# equivalent to `@derive(Debug)` with a common defuault handler
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

## built-in top-level things (idk what to call them)

# assert! takes an expression
assert! foo.bar() == 5

## stdlib

# this is stdlib print
fn print<T: Display>(value T)

# I'm honestly not yet sure which of these should be macros vs functions
print(value) # stdout, with newline
write(value) # stdout, no newline
eprint(value) # stderr, with newline
ewrite(value) # stderr, no newline
macro dbg!(expr) # debug-print, value passthru
macro assert!(expr)
```