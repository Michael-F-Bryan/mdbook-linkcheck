# Chapter 1

Here is some test $x + y$ that includes latex fragments \(z + x\).

[Some links work](./chapter_1.md)

$$
\begin{align*}
log_k(s) = d
\end{align*}
$$

Some of these fragments $(a,b,c,d,e)$ may contain something that looks like links, e.g. \([x]_5\) or $[x]_5$ or $[x](some_latex_value)$ but is, in fact, not a link at all.

[but linking to a nonexistent domain fails](http://this-doesnt-exist.com.au.nz.us/)

\[
\begin{align*}
log_k(a) = d+5 [also_not_a_link]_5 [also_not_a_link](latex_number)
\end{align*}
\]

[This chapter doesn't exist](./foo/bar/baz.html)

And sometimes the LaTeX environment is actually broken! For example, single dollar must capture only single-line latex pieces. Therefore if I'm talking about 5$ [and](first_broken_link_nonlatex)
with a dollar $ on the other line, this link should be still considered broken, and must not be erroneously cut out as a latex fragment.

Same goes for the \( single escaped parenthesis, when talking about 1000$  [this](second_broken_link_nonlatex) and [this_incomplete_link_inside_nonlatex]
must not be cut out, no matter how many $ we talk about.

[It would be bad if this worked...](../../../../../../../../../../../../etc/shadow)

[incomplete link]

![Missing Image](./asdf.png)
