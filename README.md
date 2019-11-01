# MDBook LinkCheck

[![Build Status](https://travis-ci.org/Michael-F-Bryan/mdbook-linkcheck.svg?branch=master)](https://travis-ci.org/Michael-F-Bryan/mdbook-linkcheck)
[![Build status](https://ci.appveyor.com/api/projects/status/5ysqtugw3205omc1?svg=true)](https://ci.appveyor.com/project/Michael-F-Bryan/mdbook-linkcheck)
[![Crates.io](https://img.shields.io/crates/v/mdbook-linkcheck.svg)](https://crates.io/crates/mdbook-linkcheck)
[![Docs.rs](https://docs.rs/mdbook-linkcheck/badge.svg)](https://docs.rs/mdbook-linkcheck/)
[![license](https://img.shields.io/github/license/michael-f-bryan/mdbook-linkcheck.svg)](https://github.com/Michael-F-Bryan/mdbook-linkcheck/blob/master/LICENSE)

A backend for `mdbook` which will check your links for you. For use alongside
the built-in HTML renderer.

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

## Configuration

The link checker's behaviour can be configured by setting options under the
`output.linkcheck` table in your `book.toml`.

```toml
...

[output.linkcheck]
# Should we check links on the internet? Enabling this option adds a
# non-negligible performance impact
follow-web-links = false

# Are we allowed to link to files outside of the book's root directory? This
# may help prevent linking to sensitive files (e.g. "../../../../etc/shadow")
traverse-parent-directories = false

# If necessary, you can exclude one or more web links from being checked with
# a list of regular expressions
exclude = [ "google\\.com" ]

# The User-Agent to use when sending web requests
user-agent = "mdbook-linkcheck-0.4.0"

# The number of seconds a cached result is valid for (12 hrs by default)
cache-timeout = 43200

# How should warnings be treated?
#
# - "warn" will emit warning messages
# - "error" treats all warnings as errors, failing the linkcheck
# - "ignore" will ignore warnings, suppressing diagnostic messages and allowing
#   the linkcheck to continuing
warning-policy = "warn"
```
