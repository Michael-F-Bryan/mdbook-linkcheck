# MDBook LinkCheck

A backend for `mdbook` which will check your links for you. For use alongside 
the built-in HTML renderer.

> **Warning:** Not 100% complete. May eat your laundry!

## Getting Started

Because alternate backends are still experimental, you need to install `mdbook`
directly from source instead of from crates.io.

```
$ cargo install --git https://github.com/rust-lang-nursery/mdBook
```

Then you can install `mdbook-linkcheck` (also from source).

```
$ cargo install --git https://github.com/Michael-F-Bryan/mdbook-linkcheck
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