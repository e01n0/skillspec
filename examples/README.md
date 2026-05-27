# Examples

| File | Demonstrates |
|------|-------------|
| `brainstorming.agent` | Full skill with types, lazy contexts, references, mixins, tools, permissions, pre/post contracts, and prompt directives |
| `pipeline.agent` | Multi-stage pipeline with typed data flow between skills |
| `orchestration.agent` | Multi-agent coordination with role assignments and phases |
| `tested-skill.agent` | Inline `tests {}` blocks with deterministic and LLM-judged assertions |
| `composition.agent` | Skill inheritance with `extends` and step overrides |

## Running the examples

```sh
# Type-check
skillspec check examples/brainstorming.agent

# Compile to SKILL.md
skillspec build examples/brainstorming.agent

# All examples at once
for f in examples/*.agent; do skillspec check "$f"; done
```
