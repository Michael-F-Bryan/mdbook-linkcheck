# Deeply Nested Chapter

We can link to [chapter 1](../../chapter_1.md) using relative links, but links
[relative to the root](./chapter_1.md) won't work.

Linking to a [sibling directory](./second/directory.md) relative to the source
root won't work either. We need to specify a link [relative to
here](../../second/directory.md)

This chapter is trying to detect [a bug] raised by `@mark-i-m`:

> @Michael-F-Bryan I think it only shows up with nested, relative links. For example,
>
> I just installed mdbook 0.2.1 and mdbook-linkcheck from the master branch of
> this repo (and verified that cargo used mdbook 0.2.1). No errors are reported,
> but the following link is broken:
>
> https://github.com/rust-lang-nursery/rustc-guide/blob/master/src/traits/goals-and-clauses.md#L5

[a bug]: https://github.com/Michael-F-Bryan/mdbook-linkcheck/issues/3#issuecomment-417400242
