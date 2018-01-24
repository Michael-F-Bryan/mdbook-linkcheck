# MDBook LinkCheck

A backend for `mdbook` which will check your links for you. For use alongside 
the built-in HTML renderer.

> **Warning:** Not 100% complete. May eat your laundry!


## Getting Started

First you'll need to install `mdbook-linkcheck`.

```
$ cargo install mdbook-linkcheck
```

Next you'll need to update your `book.toml` to let `mdbook` know it needs to 
use the `mdbook-linkcheck` backend.

```toml
[book]
title = "My Awesome Book"
authors = ["Michael-F-Bryan"]

[output.html]

[output.linkcheck]
```

And finally you should be able to run `mdbook build` like normal and everything
should *Just Work*.

```
$ mdbook build
```
