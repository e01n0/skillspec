# Type Inference from Field Names

When a .agent.partial file has a field with an unknown type, the field name
itself is often the strongest signal. These patterns are ordered by reliability.

## Array Indicators (high confidence)

Plural nouns almost always indicate arrays:

| Name Pattern | Inferred Type | Example |
|-------------|---------------|---------|
| `files`, `items`, `results` | `string[]` or `T[]` | `files: string[]` |
| `findings`, `issues`, `errors` | `Finding[]`, `Issue[]` | Check for a matching named type |
| `tags`, `labels`, `categories` | `string[]` | `tags: string[]` |
| `steps`, `stages`, `phases` | `T[]` | Look for a named type matching the singular |

Exception: `status` is not plural. `series` is ambiguous -- check context.

When a named type exists matching the singular form (e.g. type `Finding` exists
and the field is `findings`), use the typed array: `findings: Finding[]`.

## Integer Indicators (high confidence)

| Name Pattern | Inferred Type | Examples |
|-------------|---------------|---------|
| `count`, `total`, `num_*` | `int` | `count: int`, `num_files: int` |
| `*_count`, `*_total` | `int` | `error_count: int`, `line_total: int` |
| `line`, `column`, `offset` | `int` | `line: int` |
| `index`, `position`, `rank` | `int` | `index: int` |
| `size`, `length`, `depth` | `int` | `size: int` |
| `max_*`, `min_*`, `limit` | `int` | `max_retries: int` |
| `port`, `pid`, `exit_code` | `int` | `port: int` |

## Float Indicators (moderate confidence)

| Name Pattern | Inferred Type | Examples |
|-------------|---------------|---------|
| `score`, `rating`, `weight` | `float` | `score: float` |
| `confidence`, `probability` | `float` | `confidence: float` |
| `ratio`, `percentage`, `rate` | `float` | `ratio: float` |
| `threshold`, `tolerance` | `float` | `threshold: float` |
| `temperature`, `top_p` | `float` | Sampling parameters |

Note: `score` could be `int` if the context uses whole numbers (e.g. "out of 100").
Check surrounding prose for "0.0 to 1.0" vs "0 to 100" language.

## Boolean Indicators (high confidence)

| Name Pattern | Inferred Type | Examples |
|-------------|---------------|---------|
| `is_*`, `has_*`, `should_*` | `bool` | `is_valid: bool`, `has_tests: bool` |
| `can_*`, `will_*`, `was_*` | `bool` | `can_retry: bool` |
| `enabled`, `disabled`, `active` | `bool` | `enabled: bool` |
| `verbose`, `strict`, `quiet` | `bool` | `verbose: bool` |
| `ready`, `done`, `passed`, `failed` | `bool` | `passed: bool` |
| `*_flag`, `*_enabled` | `bool` | `debug_flag: bool` |

## String Indicators (default / moderate confidence)

| Name Pattern | Inferred Type | Examples |
|-------------|---------------|---------|
| `name`, `title`, `label` | `string` | `name: string` |
| `path`, `file`, `dir`, `url` | `string` | `path: string` |
| `message`, `description`, `summary` | `string` | `message: string` |
| `query`, `pattern`, `regex` | `string` | `query: string` |
| `*_id`, `*_key`, `*_ref` | `string` | `session_id: string` |
| `content`, `body`, `text` | `string` | `content: string` |

String is the safest fallback when no other pattern matches, but assign
lower confidence (0.4-0.5) to pure fallback inferences.

## Enum Indicators (moderate confidence)

| Name Pattern | Inferred Type | Evidence Needed |
|-------------|---------------|-----------------|
| `severity` | `enum(...)` | Look for value lists in prose |
| `status`, `state` | `enum(...)` | Look for state machine language |
| `mode`, `level`, `tier` | `enum(...)` | Look for "one of" language |
| `type`, `kind`, `category` | `enum(...)` | Look for closed set of options |
| `format`, `style` | `enum(...)` | Look for "json/markdown/plain" etc. |

Enums require finding the valid variants. Check the prose context for:
- Explicit lists: "one of: critical, high, medium, low"
- Conditional branches: "if severity is critical... if severity is high..."
- Examples that show specific values

If variants cannot be determined, use `string` with a TODO comment.

## Map Indicators (low confidence)

| Name Pattern | Inferred Type | Examples |
|-------------|---------------|---------|
| `*_map`, `*_dict`, `*_lookup` | `map<string, T>` | `config_map: map<string, string>` |
| `metadata`, `headers`, `env` | `map<string, string>` | `metadata: map<string, string>` |
| `config`, `settings`, `options` | Named type or `map<>` | Prefer named type if structure is known |

Maps are rare in .agent files. Prefer a named type when the structure is known.

## Optional Field Indicators

A field is likely optional (`?`) when:
- The name starts with a modifier: `preferred_*`, `fallback_*`, `override_*`
- The prose says "if provided", "optionally", "when available"
- Other fields in the same type are clearly required and this one is supplementary
- It has a sensible default value (add `= default` after the type)
