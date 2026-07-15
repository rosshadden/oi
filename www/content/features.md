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

## compile-time


## leading literals


## trailing functions


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

## first-class testing

Tests are a part of the language, not an afterthought.

```oi
test "division" {
	assert(div(8, 2) == 4, "should be the same")
}
```
