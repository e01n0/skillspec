---
name: formatter
input:
  text: string
  style: enum("markdown", "plain", "html") (default: "markdown")
  max_length: int
output:
  formatted: string
description: Format the input text according to the requested style.
---

## Context

Format the input text according to the requested style.
Default to markdown if no style is specified.

## Step: format

Apply the formatting rules for the chosen style and
produce the formatted output.

If max_length is set, truncate the output to that many characters
and append an ellipsis.
