# Oi

A work-in-progress systems language.
Early and unstable and thar be dragons afoot.
You have been warned.

See `examples/` for working examples, and [`ref/syntax.md`](ref/syntax.md) for more sci-fi theoretical future stuff.

> NOTE: this readme is more about building the compiler itself.
> Probably look at the website or docs or something to see something more universally helpful.

## Contributing

Requires nightly Rust (just cranelift things).

```shell
# build compiler
just build

# run tests
just test

# generate rustdocs
just doc

# runs fmt, lint, and test together, which is the combo I run most often
# (and for now is the default when you run just `just`)
just check

# preview the website (req: `zola`)
just serve
```

## Usage

```shell
# run a main.oi file in the current dir
oi run

# run a file
oi run examples/main.oi

# execute a script
oi exec '2 + 3'

# interactive REPL
oi repl
```

## Docs

- [rawsp33d.github.io/oi/](https://rawsp33d.github.io/oi/): simple website
	> NOTE: the current iteration is not yet integrated into CI, and until it is you'll see the prior (useless) version.
	> Clone and run `just serve` to see the latest docs until I get around to wiring everything up.
- [`ref/syntax.md`](ref/syntax.md): canonical language design spec
- [`ref/Oi.md`](ref/Oi.md): loose plan and direction, open questions
