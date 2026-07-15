+++
title = "about"
description = "idk what"
+++

## philosophy


### write what you mean

If a construct is obvious to both the compiler and the reader, Oi tries to let you write it directly.

- implicit returns
- leading literals
- trailing functions
- pattern matching
- named tuples
- minimal punctuation

The language tries to get the gtfo out of your way.

### everything composes


### Strong defaults


### practical

Oi borrows great ideas where they fit.

You'll see things shamelessly lifted from some languages you know well, and some you don't.
Hopefully this doesn't come off as just a grab bag build-a-lang workshop.
I think (read: hope) that Oi has its own cohesive narrative, working as more than the sum of its parts.
Some of the more specific plagiarized features (*\***cough**\* `impl` \***cough**\**) will likely get revised/renamed/reworked in the near future, but at least for now it doesn't hide its intent.

## Musings from an old man

Over my life and career I have learned and used and enjoyed a _lot_ of programming languages.
While my conventions, preferences, and style have all changed considerably over time (oh the cringe of looking at my projects from 20+ years ago...), one thing I have noticed remain largely constant is what I think of as the "good parts" and "bad parts" of languages.
I started paying more attention to these kinds of things and using those terms after reading "JavaScript: The Good Parts" by Douglas Crockford in 2008.

Like many others, I had a lot of complaints with the languages I was using.
But even at the time I was writing in enough different languages and contexts that I started seeing the good/bad parts as more of a matrix than a boolean "X is a good/bad language".

For example a lot of people see that Lua has 1-based indexes, or global variables, or `do/end` kind of blocks, or no `+=`/`-=`/`++`/`--` operators and are immediately and understandably put off.
But those people never see just how elegant Lua's tables are, how impressive its metatables are for very basic metaprogramming.
And then, because they aren't really using it, they don't see the real underlying problems past the surface-level grime: the stdlib is too barren, there's no support for basic things like POSIX or PCRE regex, it lacks classes or traits or anything of the sort, it has no switch/match statements or alternative, the ecosystem is atrocious and fragmented, modules are a joke compared to more modern languages, and there still isn't an official package management story.

And so throughout my journey I naturally began working on my own language.
Initially just a set of things I liked and didn't like, which naturally changed over time.

My first proper attempt at designing my own was _very different_ from Oi.
Codenamed `Polly` (as in polyglot) it was focused on the idea of stitching together other existing language runtimes to take advantage of the good parts of all of them.
It was (obviously?) inspired by things like shell script shebangs.
I don't think it's impossible to pull off, but I found it really hard to wrangle everything in a coherent, elegant way.
Polly mostly stayed in the design stage, though after reading [Crafting Interpreters, by Bob Nystrom](https://craftinginterpreters.com/) and implementing Lox in Zig, I did start making a pass on it before tabling the idea.

> NOTE: Bob Nystrom is prolific.
I used to subscribe to [his blog](https://journal.stuffwithstuff.com/)'s RSS feed back when that was the thing, and was so excited when his book [Game Programming Patterns](https://gameprogrammingpatterns.com/) came out.
I highly recommend everything he has ever written! Including his [post on Pratt Parsers](https://journal.stuffwithstuff.com/2011/03/19/pratt-parsers-expression-parsing-made-easy/) which I ended up coming back to for Oi as well.

More recently I have been writing a lot of [V](https://vlang.io/) in the past several years.
It too has a lot of parts I'm not in love with, but as a base foundation it is the closest I have felt to a great starting point for my internal evolving matrix of good/bad parts.

At some point I started designing what has become Oi.

## Influences

- V
- Nushell
- Rust
- random new languages that pop up on [/r/programminglanguages](https://www.reddit.com/r/ProgrammingLanguages/), like [revo](https://github.com/if-not-nil/revo)
- to a lesser extent: Nim, Zig, Clojure, Janet, Lua, Odin, Julia, Elixir, Haskell, GDScript
