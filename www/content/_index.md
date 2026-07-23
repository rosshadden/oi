+++
title = "home"
+++

A language written by human([s?](https://github.com/rawsp33d/oi/fork)), for humans.
More specifically, for humans who love programming.

Oi is a general purpose system language with a high emphasis on ergonomics.

It was designed such that the code you want to write is usually the code you actually write.
Where other languages optimize for things like safety, perf, or simplicity, Oi optimizes for [flow](https://en.wikipedia.org/wiki/Flow_(psychology)).

Its features [try to] encourage uninterrupted thought:
- expression oriented
- implicit returns
- leading literals
- trailing functions
- trailing struct literals
- pipelines
- named returns
- zero values
- destructuring
- error handling

# examples

```oi
enum Shape {
	point
	circle(f64)
	rect(f64, f64)
	triangle(f64, f64, f64)
}

fn area(s Shape) f64 {
	match s {
		.circle(r) => 3.14159 * r * r,
		.rect(w, h) => w * h,
		else => 0.0,
	}
}

Shape.rect(3.0, 4.0)
	|> area
	|> print
# => 12.0

shape := Shape.triangle(3.0, 4.0, 5.0)

# TODO: this is goofy for now because string interpolation isn't implemented yet
match shape {
	.point => {
		write("origin: ")
		print(())
	}
	.circle(r) => {
		write("circle: ")
		print((r,))
	}
	.rect(w, h) => {
		write("rect: ")
		print((w h))
	}
	.triangle(a, b, c) => {
		write("triangle: ")
		print((a b c))
	}
}
# => triangle: (3.0, 4.0, 5.0)
```
