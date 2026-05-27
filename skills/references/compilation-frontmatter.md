# Compilation: .agent to SKILL.md Frontmatter

When `skillspec build` compiles a `.agent` file, the YAML frontmatter of the
resulting SKILL.md is derived from the skill declaration, input block, and
the highest-priority context block.

## Frontmatter Fields

```yaml
---
name: skill-name
description: "First high-priority context block's text (first sentence or line)"
parameters:
  - name: field_name
    type: string
    required: true
  - name: optional_field
    type: string
    required: false
    default: "value"
---
```

## Mapping Rules

### name

Source: the string in `skill "name"`.

```agent
skill "code-review" { ... }
```
Compiles to: `name: code-review`

### description

Source: the first context block at skill-level (inside `body`), sorted by
priority descending. Takes the first sentence or the first line of prose.

```agent
body {
  context(priority: critical) {
    """
    Review code for bugs and security issues.
    Focus on the most critical findings first.
    """
  }
}
```
Compiles to: `description: "Review code for bugs and security issues."`

If no skill-level context exists, falls back to the persona text.

### parameters

Source: the `input` block. Each field becomes a parameter entry.

| .agent Syntax | Frontmatter |
|--------------|-------------|
| `query: string` | `name: query, type: string, required: true` |
| `focus?: string` | `name: focus, type: string, required: false` |
| `mode?: enum("a","b") = "a"` | `name: mode, type: enum, required: false, default: "a"` |
| `files: string[]` | `name: files, type: string[], required: true` |
| `config: MyType` | `name: config, type: object, required: true` |

Type mapping for frontmatter:

| .agent Type | Frontmatter Type |
|------------|-----------------|
| `string` | `string` |
| `int` | `integer` |
| `float` | `number` |
| `bool` | `boolean` |
| `T[]` | `T[]` (e.g. `string[]`) |
| `map<K,V>` | `object` |
| `enum(...)` | `enum` |
| Named type | `object` |

### What is NOT in frontmatter

These .agent constructs have no frontmatter representation:
- `output` block (appears in body sections instead)
- `pre` / `post` assertions
- `tools` / `permissions` (separate sections)
- `tests` (separate section)
- Named type definitions
- Step structure and dependencies

## Backporting Changes

When a SKILL.md frontmatter is modified and you need to map changes back:

| Frontmatter Change | .agent Location |
|--------------------|---------------------------------|
| `name` changed | `skill "new-name"` declaration |
| `description` changed | First sentence of highest-priority skill-level context |
| New parameter added | New field in `input { }` block |
| Parameter removed | Remove field from `input { }` block |
| Parameter type changed | Change field type in `input { }` block |
| `required` flipped to false | Add `?` to field: `field?: type` |
| `required` flipped to true | Remove `?` from field |
| `default` added/changed | Add/change `= value` after type |

Caution: changing description in frontmatter should ONLY modify the first
sentence of the context block. Do not replace the entire context block.
