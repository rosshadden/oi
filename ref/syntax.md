# [[Oi|../]]

```rust
## comments

# Single line comments
# (can be stacked)

#{ Block comments
	#{ (can be nested) }
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
	assert(x == $)
}
fn one_tuple(x int,) {
	assert(x == $.0)
}
fn two_tuple(x int, y int) {
	assert(x == $.0)
	assert(y == $.1)
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
	assert(result == 0)
	result = 2
	return
}
assert(two() == 2)

fn random_user() u User {
	print(u) # User{}
	u.name = "I Dunno"
}
ru := random_user()
assert(ru.name == "I Dunno")

# this really just skips the step of explicitly initializing a zeroed var
fn divmod(a int, b int) out (int, int) {
	out.0 = a / b
	out.1 = a % b
	return
}

## leading literals

# if there's only one literal arg, the parens may be dropped
foo "bar"
print "lol"
sleep 1_000
log.group :process

# this can be used in conjunction with trailing blocks
test "foo" { assert(foo == bar) }
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
assert profile.foo == profile.Options.foo

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
assert(Point{1, 0} + Point{2, 3} == Point{3, 3})

## traits

struct Dog {
	kind string
}
impl Dog {
	fn speak(self) string { "woof" }
}

struct Cat {
	kind string
}
impl Cat {
	fn speak(self) string { "meow" }
}

trait Animal {
	kind string
	speak() string
}
impl Animal {
	# traits can have their own methods that use other defined fields and methods
	fn shout(self) string {
		self.speak().upper()
	}
}

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
assert(Person is of Animal)

# you can optionally make traits implicit with @implicit
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
assert(Kiwi is of Fruit)
assert(Apple is of Fruit)
assert(Bike is not of Fruit)

## main entrypoint

fn main() {
	## variables
	
	# assignment
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
	assert(names[1] == "jacob")
	i := 1
	assert(names[i] == names[1])
	# numbers literals may also be used with dot notation
	assert(names.0 == "john")
	assert(names.2 == "jingleheimerschmidt")
	
	# append with `<<`
	mut odd := [1, 3, 5]
	odd << 7
	assert(odd.3 == 7)
	# entire arrays can be appended too
	odd << [9 11]
	assert(odd.5 == 11)
	assert(odd.len == 6)
	
	# arrays support dropping the commas when only literals are present
	even := [2 4 6]
	
	# `in` operator returns whether array contains element
	assert(6 in even)
	
	# arrays have fields
	# `len` is the number of initialized elements in the array
	assert(even.len == 3)
	
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
	assert even[1..3] == [2 4]
	assert even[..3] == [0 2 4]
	assert even[1..] == [2 4 6 8]
	
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
	assert(t.a == t.0)
	assert(t.b == t.1)
	
	#{
		These names are purely aliases / hints, and do _not_ affect identity or comparison.
		Think of it like somebody asks us if their rock is the same as our rock.
		We can tell that they are the same, we just happen to know a lot more details about our rock than theirs.
		I've never been great with analogies.
		Anyway don't abuse this. The field names are for convenience, not as a replacement for structs.
	}#
	assert((x: 4, y: 2) == (4, 2))
	assert((x: 4, y: 2) == (4, z: 2))
	
	# names do not need to be given to all indices
	t := (1, b: 2)
	print(t) # (1, b: 2)
	assert(t.b == t.1)
	
	# can be used in function return signatures
	fn split(value string) (left string, right string) {
		split_once(value, "|") # returns a 2-tuple (a twople? anyone?)
	}
	splat := split("hi|mom")
	(l, r) := split("hi|mom")
	assert(splat.left == "hi")
	assert(splat.right == "mom")
	assert(splat == (l, r))
	
	# another example with a common divmod method
	fn divmod(a int, b int) (q int, r int) {
		(a / b, a % b)
	}
	result := divmod(10, 3)
	print(result) # (q: 3, r: 1)
	assert(result == (3, 1))
	assert(result.0 == 3)
	assert(result.1 == 1)
	assert(result.q == 3)
	assert(result.r == 1)

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
	assert(result.body == result.1)
	
	## atoms
	
	# Oi has first-class atoms
	:foo
	assert(:foo != :bar)
	food := :apple
	assert(food == :apple)
	
	# atoms are contextually coerced into Enum values
	# (this hopefully reminds the reader of the `.value` enum shorthand in Zig and V)
	enum Stat { health mana stamina }
	struct User {
		mut stat Stat
	}
	user1 := User{ Stat:mana } # this is normal enum syntax
	user2 := User{ :mana } # this is a coerced atom
	assert(user1.stat == user2.stat)
	# another context that coerces
	assert(Stat:stamina == :stamina)
	
	# this might be useful for quick prototyping to organically become finalized
	
	# prototype code
	mut state := :loading
	state = :ready
	
	# on a later pass, despite nothing at the callsites changing, adding this enum definition would add strong typing and copiler checking
	enum State { loading ready error }
	
	## types
	
	# type aliases
	type Score = int
	
	# type aliases are defined as tuples
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
	# lambda function
	print(op(4, |n| 4 * n)) # 16
	
	# all types have zeroed values
	u := User{}
	assert(u.age == 0)
	assert(u.name == "")
	
	# unpack returns
	a, b := foo()

	## loops
	
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
	
	## [almost?] everything is an expression
	
	# ternary (`if` is an expression)
	foo := if true { "yes" } else { "no" }

	# built-in functions
	result := assert(check()) |> next
	
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
	
	# ?T and !T must be handled; the or block is required to unwrap
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
	mut c := Color.green
	c = .red
	c = :blue
	
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
		assert(true, "optional message")
		panic("uh oh...")
	}
	
	## blocks
	
	# blocks are groups of expressions
	# the final expression is the result of the block
	three := {
		light_the_beacons()
		3
	}
	
	# blocks may use `;` to indicate joined lines
	long_but_short := { do_thing(); 3 }
	
	# normally blocks are eager, but when directly passed to something that expects a callable, they are deferred and treated as callables
	nums.map({ $ * 2 })
	
	# trailing blocks
	
	# if a block is the last argument of a function, it may be placed outside the parens
	spy() {
		dump($)
		$
	}
	
	# if a leading literal and/or trailing block were the only params of a function, the parens may be omitted entirely
	spy {
		dump($)
		$
	}
	
	# all of this intentionally composes with leading literals,
	# allowing for ordinary functions that look like they are built-in
	# ```
	# test("registration", {
	#	 user := make_user()
	#	 assert(user.can_register())
	# })
	# ```
	test "registration" {
		user := make_user()
		assert(user.can_register())
	}
	retry 3 { fetch(url)! }
	timeout 5.sec { slow_call() }
	
	# `$` refers to the input args as stated above, which in this case is whatever the function calls such blocks with
	db.transaction {
		$.insert(user)
		$.insert(order)
	}

	# the input data can be bound to a name when desired
	# this lets you handle nested blocks
	# db.transaction({|tx| ...})
	db.transaction {|tx|
		tx.insert(user)
		tx.insert(order)
	}
	
	# mutex.with({ do_work() })
	mutex.with {
		do_work()
	}
	
	## closures

	# anonymous functions
	
	adder := fn (n int) int { n + n }
	spawn(fn () { do_work() })
	
	# optional capture list, defaulting to [] when not provided
	mut counter := 0
	increment := fn [mut counter] (x int) int {
		counter += x
		counter
	}
	
	## lambdas
	# captures immutable bindings, by reference

	# inline form
	double := |x| x * 2
	nums.map(|x| x * 2)
	sort_by(|a, b| a.age < b.age)
	spawn(|| do_work())
	
	# blocks may be used for the body (since blocks are expressions)
	process := |x| {
		y := validate(x)!
		y * 2
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
	
	# `$` is the data flowing into the pipeline step
	# this lets us do clojure-like threading
	"threading"
	  |> wrap("[", $, "]")
	  or log_errors("foo", $)
	"hello" |> $ + " world"
	[2 4 6 8] |> if $.len() > 0 { print(true) }
	
	# any errors in the pipeline flow directly to an `or`
	"error-only pipes"
		|> upper
		or handler
	
	# pipeline steps can be blocks too
	result := "error-only pipes with block"
		|> {
			idk($)
		}
		|> {
			log.info("stuff and things: {$}")
			true
		}
		or {
			eprint($)
			return $
		}
	config := os.env("config_path")
		|> read_file!
		|> parse!
		or {
			log.warn("Config load failed: {$}. Using default.")
			default_config()
		}
	"gtfo" |> process or { panic("uh oh...") }
	"err binding" |> raise_err |> or {|err| log.error(err) }
	
	# input data can be optionally bound to names when desired
	# this lets you unambiguously nest
	"foo" |> {|outer|
		assert(outer == $)
		outer |> {|inner|
			assert(inner == $)
			log.debug("inner: {inner}, outer: {outer}")
		}
		assert(outer == $)
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
	assert(count_letters("hi, mom!") == 4)
	
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
					$(fields.map(|f| quote { self.$f == other.$f }).join(" && "))
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

# assert takes an expression
assert foo.bar() == 5

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