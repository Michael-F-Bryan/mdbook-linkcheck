# MDBook Link-Check

[![Continuous integration](https://github.com/Michael-F-Bryan/mdbook-linkcheck/workflows/Continuous%20integration/badge.svg?branch=master)](https://github.com/Michael-F-Bryan/mdbook-linkcheck/actions)
[![Crates.io](https://img.shields.io/crates/v/mdbook-linkcheck.svg)](https://crates.io/crates/mdbook-linkcheck)
[![Docs.rs](https://docs.rs/mdbook-linkcheck/badge.svg)](https://docs.rs/mdbook-linkcheck/)
[![license](https://img.shields.io/github/license/michael-f-bryan/mdbook-linkcheck.svg)](https://github.com/Michael-F-Bryan/mdbook-linkcheck/blob/master/LICENSE)

A backend for `mdbook` which will check your links for you. For use alongside
the built-in HTML renderer.

## Getting Started

First you'll need to install `mdbook-linkcheck`.

```
cargo install mdbook-linkcheck
```

If you don't want to install from source (which often takes a while) you can
grab an executable from [GitHub Releases][releases] or use this line of
`curl` to download a release bundle and install it in the `./mdbook-linkcheck`
directory:

```console
mkdir -p mdbook-linkcheck && cd "$_" && \
  curl -L https://github.com/Michael-F-Bryan/mdbook-linkcheck/releases/latest/download/mdbook-linkcheck.x86_64-unknown-linux-gnu.zip -o mdbook-linkcheck.zip && \
  unzip "$_" && \
  chmod +x mdbook-linkcheck && \
  export PATH=$PWD:$PATH && \
  cd ..
```

(note: you may need to replace the `x86_64-unknown-linux-gnu` with your
platform's target triple)

Next you'll need to update your `book.toml` to let `mdbook` know it needs to
use `mdbook-linkcheck` as a backend.

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

> **Note:** When multiple `[output]` items are specified, `mdbook` tries to
> ensure that each `[output]` gets its own sub-directory within the `build-dir`
> (`book/` by default).
>
> That means if you go from only having the HTML renderer enabled to enabling
> both HTML and the linkchecker, your HTML will be placed in `book/html/`
> instead of just `book/` like before.

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

# If necessary, you can exclude one or more links from being checked with a
# list of regular expressions. The regex will be applied to the link href (i.e.
# the `./index.html` in `[some page](./index.html)`) so it can be used to
# ignore both web and filesystem links.
#
# Hint: you can use TOML's raw strings (single quote) to avoid needing to
# escape things twice.
exclude = [ 'google\.com' ]

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

# Extra HTTP headers that must be send to certain web sites
# in order to link check to succeed.
#
# This is a dictionary (map), with keys being regexes
# matching a set of web sites, and values being an array of
# the headers.
[output.linkcheck.http-headers]
# Any hyperlink that contains this regexp will be sent
# the "Accept: text/html" header
'crates\.io' = ["Accept: text/html"]

# mdbook-linkcheck will interpolate environment variables into your header via
# $IDENT.
#
# If this is not what you want you must escape the `$` symbol, like `\$TOKEN`.
# `\` itself can also be escaped via `\\`.
#
# Note: If interpolation fails, the header will be skipped and the failure will
# be logged. This can be useful if a particular header isn't always necessary,
# but may be helpful (e.g. when working with rate limiting).
'website\.com' = ["Authorization: Basic $TOKEN"]
```

## Continuous Integration

Incorporating `mdbook-linkcheck` into your CI system should be straightforward
if you are already [using `mdbook` to generate documentation][mdbook-ci].

For those using GitLab's built-in CI:

```yaml
generate-book:
  stage: build
  image:
    name: michaelfbryan/mdbook-docker-image:latest
    entrypoint: [""]
  script:
    - mdbook build $BOOK_DIR
  artifacts:
    paths:
      - $BOOK_DIR/book/html
    # make sure GitLab doesn't accidentally keep every book you ever generate
    # indefinitely
    expire_in: 1 week

pages:
  image: busybox:latest
  stage: deploy
  dependencies:
    - generate-book
  script:
    - cp -r $BOOK_DIR/book/html public
  artifacts:
    paths:
    - public
  only:
    - master
```

The [michaelfbryan/mdbook-docker-image][image] docker image is also available
on Docker hub and comes with the latest version of `mdbook` and
`mdbook-linkcheck` pre-installed.

[releases]: https://github.com/Michael-F-Bryan/mdbook-linkcheck/releases
[mdbook-ci]: https://rust-lang.github.io/mdBook/continuous-integration.html
[Michael-F-Bryan]: https://github.com/Michael-F-Bryan
[image]: https://hub.docker.com/r/michaelfbryan/mdbook-docker-image
