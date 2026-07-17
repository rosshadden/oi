# [[Oi|../]]

Things I'm playing with that might not work or make it.

```rust
# normie
fn add(a int, b int) int {
	return a + b
}

# implicit return
fn add(a int, b int) int {
	a + b
}

# normie named
fn add(a int, b int) out int {
	out
}

# implicit return named
fn add(a int, b int) int {
	a + b
}

# normie tuple
fn passthru(a int, b int) (int, int) {
	return (a, b)
}

# implicit tuple
fn passthru(a int, b int) (int, int) {
	(a, b)
}

# normie multiple return
fn passthru(a int, b int) (int, int) {
	return a, b
}

# implicit multiple return
fn passthru(a int, b int) (int, int) {
	a, b
}

# named tuple
fn passthru(a int, b int) (c int, d int) {
	c = a
	d = b
	return
}

# $out
fn passthru(a int, b int) (int, int) {
	$out.0 = a
	$out.1 = b
	return
}

# $out
fn passthru(a int, b int) out (c int, d int) {
	out.c = a
	out.d = b
	return
}

fn new_dude(name string) Dude {
	Dude{
		name: name
	}
}

fn new_dude(name string) d Dude {
	d.name = name
	d
}
```