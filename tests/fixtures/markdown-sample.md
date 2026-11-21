# Rendered Markdown sample

This paragraph is one long line with **bold**, *italic*, ***both***, `inline code`, a [link with a long address](https://example.com/a/b_c_d?x=1&y=2), and a bare address, https://example.org/path_with_underscores. It keeps going so the view has to wrap it on screen, while the file keeps it as a single line, the way it was typed.

## Lists

- First item with **bold**
- Second item with a [link](https://example.com)
  - Nested item
1. Numbered
2) Another style
- [ ] An open task
- [x] A finished task

### Quote and code

> A quoted line with *emphasis*.
> > A nested quote.

```rust
fn main() { let **not_bold** = [not](a_link); }
```

~~~
Tilde fence with *no emphasis*
~~~

| Column | **Bold head** | Right |
|:-------|:-------------:|------:|
| one | `code` | 1 |
| two | [link](https://example.com) | 22 |

---

Escaped \*stars\* stay literal, snake_case_words stay plain, and 2 * 3 * 4 is math.
