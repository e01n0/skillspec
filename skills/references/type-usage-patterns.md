# Type Inference from Prose Usage

When field names are ambiguous, the prose context where a field is referenced
gives strong signals about its type. Scan step contexts, skill-level contexts,
and examples for these patterns.

## Array Signals

Language that implies iteration or collection operations:

| Prose Pattern | Inferred Type | Confidence |
|--------------|---------------|------------|
| "iterate over", "for each", "loop through" | `T[]` | high |
| "list of", "collection of", "set of" | `T[]` | high |
| "one or more", "multiple", "several" | `T[]` | high |
| "filter", "select from", "search through" | `T[]` | high |
| "combine all", "aggregate", "merge" | `T[]` | moderate |
| "add to", "append", "push" | `T[]` | moderate |
| "first", "last", "nth", "any of" | `T[]` | moderate |
| "empty list", "no items", "when there are none" | `T[]` | high |
| "count of", "number of items in" | field is `T[]`, not `int` | moderate |
| "sort by", "order by", "rank" | `T[]` | moderate |

The element type `T` can often be inferred from the same prose:
- "list of file paths" -> `string[]`
- "collection of findings" -> `Finding[]` (if type exists)
- "set of scores" -> `float[]` or `int[]`

## Enum Signals

Language that implies a closed set of choices:

| Prose Pattern | Inferred Type | Confidence |
|--------------|---------------|------------|
| "one of: X, Y, Z" | `enum("X", "Y", "Z")` | high |
| "choose between X and Y" | `enum("X", "Y")` | high |
| "either X or Y" | `enum("X", "Y")` | high |
| "must be X, Y, or Z" | `enum("X", "Y", "Z")` | high |
| "categorise as" + finite list | `enum(...)` | high |
| "if [field] is X ... if [field] is Y" | `enum("X", "Y", ...)` | moderate |
| "switch on", "based on the type" | `enum(...)` | moderate |
| "valid values are" | `enum(...)` | high |

Extract the exact variant strings from the prose. If the prose uses
inconsistent casing ("High" vs "high"), normalise to lowercase for the enum.

## Integer Signals

Language that implies whole numbers or counting:

| Prose Pattern | Inferred Type | Confidence |
|--------------|---------------|------------|
| "how many", "number of", "count" | `int` | high |
| "at most N", "at least N", "exactly N" | `int` | high |
| "increment", "decrement" | `int` | high |
| "zero or more", "one or more" (as quantity) | `int` | moderate |
| "times", "attempts", "retries" | `int` | high |
| "line N", "position N" | `int` | high |
| "index into", "offset" | `int` | high |

## Float Signals

Language that implies continuous values or ratios:

| Prose Pattern | Inferred Type | Confidence |
|--------------|---------------|------------|
| "between 0 and 1", "0.0 to 1.0" | `float` | high |
| "percentage", "fraction", "proportion" | `float` | high |
| "weighted", "scaled" | `float` | moderate |
| "probability", "likelihood" | `float` | high |
| "rate of", "ratio of" | `float` | high |
| "score from 0 to 1" | `float` | high |
| "score from 0 to 100" | `int` | moderate |
| "threshold above/below" | `float` | moderate |

## Boolean Signals

Language that implies binary state:

| Prose Pattern | Inferred Type | Confidence |
|--------------|---------------|------------|
| "whether or not", "if enabled" | `bool` | high |
| "true/false", "yes/no" | `bool` | high |
| "flag", "toggle", "switch" | `bool` | high |
| "is it", "does it", "has it" | `bool` | high |
| "on/off", "active/inactive" | `bool` | high |
| "only when [field]", "if [field]" (as guard) | `bool` | moderate |
| "skip if", "include when" | `bool` | moderate |

## String Signals

Language that implies freeform text (often the default):

| Prose Pattern | Inferred Type | Confidence |
|--------------|---------------|------------|
| "describe", "explain", "summarise" | `string` | moderate |
| "the path to", "URL of" | `string` | high |
| "name of", "title of" | `string` | high |
| "message", "error text" | `string` | high |
| "content of", "body of" | `string` | moderate |
| "regex pattern", "glob pattern" | `string` | high |

## Map Signals

Language that implies key-value lookups:

| Prose Pattern | Inferred Type | Confidence |
|--------------|---------------|------------|
| "look up by", "keyed by", "indexed by" | `map<K, V>` | moderate |
| "mapping from X to Y" | `map<X, Y>` | high |
| "dictionary of", "table of" | `map<string, T>` | moderate |
| "environment variables" | `map<string, string>` | high |
| "headers" (HTTP context) | `map<string, string>` | high |

## Named Type Signals

When prose describes a structured entity with multiple attributes:

| Prose Pattern | Signal | Action |
|--------------|--------|--------|
| "each finding has a file, line, and severity" | Named type | Define `type Finding { file: string  line: int  severity: ... }` |
| "the result includes X, Y, and Z" | Named type | Define a result type with those fields |
| "a record containing" | Named type | Define a type for the record |

Prefer named types when:
- Three or more attributes are described together
- The same structure is referenced in multiple places
- The entity has a natural name ("Finding", "Change", "Result")

Use a primitive when:
- Only one attribute matters
- The structure is used exactly once
- A named type would be a single-field wrapper

## Combining Signals

When name and usage disagree, usage wins:
- Field named `result` (suggests string) but prose says "list of findings" -> `Finding[]`
- Field named `items` (suggests array) but prose says "number of items" -> `int` (rename to `item_count`)

When multiple usage patterns conflict, assign lower confidence and leave a TODO.
