# Deeply nested stress fixture

Nested lists, blockquotes, and a table for §4.4 stress.

- L1 item
  - L2 item
    - L3 item
      - L4 item
        - L5 item
          - L6 item
            1. L7 ordered
               1. L8 ordered
                  - L9 bullet
                    - [ ] L10 task

> Q1 blockquote
> > Q2 nested
> > > Q3 nested
> > > > Q4 nested
> > > > > Q5 nested
> > > > > > Q6 nested

| Col A | Col B | Col C |
|-------|-------|-------|
| a `code` here | **bold** and *italic* | [link](https://example.com) |
| > quote in cell | nested `- list`? | ~~strike~~ |
| deeply | nested | content |

## After the stress block

Plain paragraph to confirm the document recovers after the nested content.
