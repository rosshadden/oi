+++
title = "features"
+++

## [almost] everything is an expression

It is a goal for absolutely everything to be an expression.
Right now most things are, like in Rust.
```oi
# if expressions
status := if score > 100 {
	"absolute legend"
} else {
	"noob"
}

# match expressions
value := match token {
	.number(n) { n }
	.ident(_) { 0 }
}
```

## pipelines

```oi
# basic
call_to_action := "let's do this" |> trim |> upper

# option/result aware
# short-circuits on none/error
nickname := find_user(id)
	|> get_profile?
	|> get_display_name?
	or "anonymous"
```

## error handling

Errors are values, and recovery is explicit.

```oi
user := load_user(id) or {
	return error("unknown user")
}
```

## leading literals

If there's only one literal argument, function parens may be omitted.

```oi
print "lol"
foo "bar"
sleep 1_000
log.group :process
```

## trailing struct literals


## trailing functions

```oi
# if a function is the last argument of a call, it may be written after the parens.
retry(3) fn {
	fetch(url)?
}

# if no named params are needed, the `fn` may be omitted
retry(3) {
	fetch(url)?
}

# if the trailing function is the only argument, the parens may be omitted too
spawn {
	do_work()
}
```

## implicit input

> TODO: rename to something cooler?

`$` is the data passed to a function.

```oi
fn print_coord(x int, y int) {
	print($.0, $.1)
}
```

This is especially useful when using inside pipelines.

```oi
# clojure-like threading
"hello" |> $ + " world"
"goodbye" |> wrap("[", $, "]")
```

## named returns

Bindings may be provided to return signatures, creating a mutable zeroed value.

```oi
fn divmod(a int, b int) out (int, int) {
	out.0 = a / b
	out.1 = a % b
	return
}
```

## first-class testing

Tests are a part of the language, not an afterthought.

```oi
test "division" {
	assert(div(8, 2) == 4, "should be the same")
}
```

## comfy tooling

CLI commands you're used to.

```bash
oi
oi repl
oi run .
oi exec "2 + 5"

# not yet implemented:
oi fmt
oi init
oi test
oi doc
oi build
oi watch
oi lsp
```
