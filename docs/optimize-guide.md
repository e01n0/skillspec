# Optimize Guide

`skillspec optimize` uses [SkillOpt](https://github.com/microsoft/SkillOpt) to iteratively improve your skills via LLM-driven reflection. All LLM calls route through your hosting agent session — no external API keys needed.

## Setup

```sh
skillspec optimize --setup my-skill.agent
```

This creates a Python venv in `optimize/`, clones SkillOpt, and installs dependencies. One-time operation.

## Workflow

### 1. Prepare

```sh
skillspec optimize my-skill.agent --prepare
```

Compiles the initial SKILL.md and exports train/test data splits from your `tests {}` blocks. Creates a working directory at `<skill>.optimized/`.

### 2. Run training

```sh
skillspec optimize my-skill.agent
```

Runs the full training loop (default: 3 epochs). Each step:
- Reflects on test results
- Proposes edits to the SKILL.md (context text, priorities, step instructions)
- Evaluates the changes
- Keeps improvements, reverts regressions

### 3. Write back to source

```sh
skillspec optimize my-skill.agent --writeback
```

Applies the optimised SKILL.md changes back to your `.agent` source file. Use `--no-overwrite` to write to `.agent.optimized` instead.

## Tuning knobs

| Flag | Default | Effect |
|------|---------|--------|
| `--epochs` | 3 | Number of training passes |
| `--batch-size` | 4 | Test cases per reflection batch |
| `--edit-budget` | 5 | Max edits per step (the "learning rate") |
| `--scheduler` | constant | Edit budget schedule: `constant`, `linear`, or `cosine` |

Lower `--edit-budget` for conservative refinement. Higher for aggressive exploration.

## Resuming

If training is interrupted, resume from the last checkpoint:

```sh
skillspec optimize my-skill.agent --resume <checkpoint.json>
```

## Step-by-step mode

For manual control, run one step at a time:

```sh
skillspec optimize my-skill.agent --step
```

This returns an LLM request. Feed the response back with `--response`.
