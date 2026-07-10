# Tenetic — product specification

**Tenetic is the consistency and drift control plane for AI agent rulebooks.**
It extracts every rule your agents follow — from SKILL.md files, CLAUDE.md,
Cursor rules, AGENTS.md, system prompts — detects conflicts, drift, and
shadowing across the whole fleet, pins the accepted state in a lockfile, and
probes how models actually adjudicate the tensions.

*A tenet is a rule you hold. Tenetic makes sure your agents hold the same ones.*

> Status: specification. Seeds a new repository; harvests specific modules
> from [skillspec](https://github.com/e01n0/skillspec) (see §12). Name
> "Tenetic" is clear on crates.io / npm / PyPI; a media-analytics company
> uses the bare word (different class), so run a class 9/42 trademark check
> and secure a distinct domain (e.g. tenetic.dev) before public launch.

---

## 1. The problem

Teams now run fleets of interdependent agent skills: dozens of documents,
hundreds of imperative rules, edited by multiple people and by agents
themselves. Three failure modes, none visible today:

- **Conflict.** Rule 47 in one skill contradicts rule 212 in another. Both
  load into the same context. The model silently picks one — which one
  depends on position, phrasing, and model version.
- **Drift.** You fix a rule in one place; stale copies survive in two
  others. Catching this by hand means re-reading the entire fleet after
  every edit. (Origin story: a full weekend lost to debugging hundreds of
  conflicting rules across interdependent skills — the fix worked, but
  nothing could confirm it *stayed* fixed.)
- **Erosion & behavioral drift.** A critical rule quietly disappears, or the
  text stays identical but a model upgrade changes which rule wins a
  tension. No text diff can see either.

Every rule collection is a program with no compiler. Per-file linting exists
(and is solved); the *cross-document semantic layer* has no tooling — the
research survey (§13) found no paper, benchmark, or product that
consistency-checks agent rule collections.

## 2. Product thesis

1. **No new language, no migration.** Markdown stays the source of truth.
   Tenetic reads what teams already have. Adoption cost of the first scan:
   zero. (Decision record: Tenetic's predecessor, SkillSpec, was a typed DSL
   compiling to SKILL.md. The operational loops turned out to be the value;
   the language was the toll booth. Tenetic inverts that: loops first,
   optional structure later — carried as frontmatter annotations, never as
   a required syntax.)
2. **Deterministic first.** The core engine uses no models and no network:
   reproducible in CI, millisecond-fast, zero API cost. Learned tiers
   (embeddings, local NLI) are opt-in and local; LLM judgment appears in
   exactly one place — behavioral probes — where the trace is evidence, not
   opinion. Grounded in verified literature (§13): local pipelines beat
   zero-shot GPT-4o on domain rule text; LLM-only conflict detection scored
   0% recall in the one published head-to-head.
3. **State, not snapshots.** `tenetic.lock` is the Terraform move: a pinned
   record of the fleet's semantic state (known findings, accepted
   exceptions, probe verdicts). CI fails only on *changes* to that state,
   so fixed conflicts stay fixed and triaged noise stays triaged.

## 3. Core concepts

| Concept | Definition |
|---|---|
| **Tenet** | An atomic imperative rule extracted from a source document, with provenance (file, line, owning skill), polarity, subject keywords, and — where declared — priority, scope, and version. |
| **Source** | Any document contributing tenets: SKILL.md, CLAUDE.md / AGENTS.md, `.cursorrules`/`.mdc`, system-prompt files, SkillSpec `.agent` (supported via adapter). |
| **Role / tier** | The disclosure tier a tenet lives at: **description** (frontmatter — always in context), **body** (`SKILL.md` — loaded when the skill triggers), **reference** (sibling files — loaded on demand), or **root** (org policy). Sets how 'hot' a rule is: conflict severity and leanness budget both scale with tier. |
| **Stratum** | The precedence level of a source. Root documents (org policy, CLAUDE.md) outrank skills; a skill tenet contradicting a root tenet is a *policy violation*, not a peer conflict. |
| **Scenario** | A co-activation set: the sources that can be loaded into one context simultaneously (derived from triggers, targets, and path scopes). Tenets that never co-activate cannot conflict. |
| **Rulebook** | The *effective rulebook* of a scenario: the serialized, ordered, precedence-resolved sequence of tenets the model actually sees. The unit of analysis for ordering checks. |
| **Finding** | A classified relationship between tenets (taxonomy below) with a stable content-hash identity that survives line moves and reformatting. |
| **Lock** | `tenetic.lock`: accepted findings, tenet inventory fingerprint, probe verdicts. The semantic state file. |
| **Probe** | A behavioral experiment: compose the scenario's rulebook, pose a task that forces a flagged tension, run N times, judge which tenet the model obeyed. |
| **Resolution** | A *gated* edit that clears a finding: proposed (deterministic, native-LLM, or SkillOpt), re-checked by the detectors and a probe, then pinned in the lock. |

### Finding taxonomy

Adapted from firewall-policy conflict analysis (shadowing / generalization /
correlation / redundancy) plus the drift classes:

| Kind | Meaning | Tier |
|---|---|---|
| `duplicate` | Identical tenet in multiple places | 0 |
| `drifted-duplicate` | Near-identical copies that diverged | 0 |
| `cross-tier-duplicate` | Same rule in a skill's body *and* its reference (or description) — the drift trap inside one skill; pick a tier | 0 |
| `redundant-with-root` | A skill rule restates a root-policy rule; drop it and inherit, or it drifts from policy | 1 |
| `unmarked-exception` | Specific tenet reverses a general absolute stated elsewhere, with no linkage between them | 1 |
| `shadowing` | Ordering/priority makes a specific tenet unreachable behind its general rule | 1 |
| `ambiguity` | Opposite polarity, shared subject, neither more specific — a genuine contradiction | 1 |
| `priority-mismatch` | Near-identical tenets declared at different priorities | 1 |
| `policy-violation` | Skill tenet contradicts a root-stratum tenet | 1 |
| `dangling-reference` | Tenet references a skill/artifact that no longer exists | 2 |
| `inlined-copy` | One skill restates another's process inline, now stale relative to it | 2 |
| `ordering-contradiction` | Incompatible before/after/then relations on shared actions across sources | 2 |
| `erosion` | A critical-priority tenet disappeared from the fleet | 0 |
| `behavioral-flip` | A probed tension whose verdict distribution changed since last pinned (model upgrade, fleet composition change) — text identical | probe |

### Quality findings (advisory by default — see §6)

| Kind | Meaning |
|---|---|
| `bloated-skill` | `SKILL.md` body past a token/line threshold |
| `bloated-description` | Oversized frontmatter description (the always-on tier — worst place to waste tokens) |
| `no-progressive-disclosure` | A long skill with zero references — monolithic |
| `inline-reference` | Reference-grade detail (schemas, long examples, procedures) inlined in the body; demote to a reference file |
| `orphan-reference` | A reference file nothing links to — dead weight or broken intent (flagged, not silent) |
| `reference-is-really-a-skill` | A "reference" file with its own frontmatter/triggers; promote it |
| `emphasis-overuse` | Density of MUST / NEVER / DO NOT / ALL-CAPS / bold above threshold — when everything is emphasized, nothing is |
| `weak-description` | Description that doesn't say *when* to use the skill (it is the trigger) |
| `dead-reference` | A link to a file that doesn't exist (shared with the graph tier) |

## 4. Product surface

```sh
tenetic scan  [path]              # extract tenets + report findings (read-only)
tenetic review [path]            # quality: leanness, disclosure, emphasis, hygiene (per-skill)
tenetic check [path]              # CI gate: fail on findings not in tenetic.lock
tenetic baseline [path]           # pin current findings as accepted state
tenetic graph [path] --format dot|mermaid   # reference/artifact/ordering graph
tenetic probe [finding-id|--new]  # behavioral probes via hosting agent session
tenetic diff <old> <new>          # semantic diff of two fleet states; semver class
tenetic explain <finding-id>      # full provenance, both tenets in context, why flagged
tenetic fix   [finding-id|--all]  # propose + gate + verify a resolution, then pin it
```

- **CI:** a published GitHub Action (`uses: <org>/tenetic@v1`) running
  `check` on every PR touching rule files. This is the product's primary
  distribution channel — "the failing check that saved your weekend" is the
  growth loop.
- **Watch mode:** `tenetic check --watch` for local editing sessions.
- **Output contracts:** human-readable report; `--json` for editor/agent
  integration (agents editing skills should run Tenetic themselves —
  a Claude Code hook recipe ships in the docs).

### Suppression & triage UX

The literature's validated precision ceiling for deterministic detection is
~84%. Therefore, first-class:
- `baseline` accepts everything current (adopt-in-anger path);
- per-finding accept with a reason (`tenetic accept <id> --reason "…"`),
  stored in the lock, surfaced by `explain`;
- finding identity is content-hashed from the tenet texts: editing either
  side of an accepted pair re-opens it automatically. No silent decay of
  the baseline.

## 5. Architecture

```
sources (md / mdc / .agent / prompts)
   │  format adapters
   ▼
tenet extraction  ── sentence rejoin, bullet/prose split, imperative
   │                 filtering, polarity, keyword normalization, tier tagging
   ▼
scenario builder  ── strata, triggers, targets → co-activation sets
   ▼                 + effective-rulebook serialization (order, priority)
tier 0/1  deterministic detectors (always on; no models, no network)
   ▼
tier 2    graph detectors: references, artifacts, ordering (deterministic)
   ▼
tier 3    [--semantic] local models: embedding candidate pairing +
   │       NLI cross-encoder (ONNX, CPU, offline; gated on a benchmarked
   │       eval set of real skill conflicts — see risks)
   ▼
probe     [on demand] hosting-agent transport: scenario rollouts + narrow
   │       trace-verdict judging; verdicts pinned in the lock
   ▼
tenetic.lock  ←→  check / diff / report
```

Implementation: Rust core (harvested — §12), shipped as both a single
static binary and a PyO3/maturin Python wheel (§8) from one codebase. No
runtime dependencies for tiers 0–2. Tier 3 models load lazily behind a
flag. Probes require only a hosting agent session (Claude Code, Cursor) or
a model-serving endpoint, zero API keys, via the checkpoint-resume
request/response protocol.

Two deterministic detector families run over the same extracted,
tier-tagged rules: **consistency** (between rules — conflicts, drift,
duplication; tiers above) and **quality** (within a skill —
best-practice review; §6). Both feed one `tenetic.lock`.

### The probe tier: behavioral confirmation, not detection

The optional probe tier is the *only* place an LLM enters the pipeline, and
its job is deliberately narrow. Probes do not **detect** conflicts — the
deterministic tiers do that. Probes **confirm** an already-flagged conflict,
upgrading it from *suspected* to *confirmed live* when a trace shows the
model actually sacrificing one rule for the other under a task that forces
the tension. A pair the model reconciles gracefully every run is down-ranked
to a nit; a pair whose behavior flips on phrasing is a live fault. This is
what gives findings a *severity*, and what `behavioral-flip` re-checks over
time (a model upgrade can turn a settled pair live without a word of text
changing).

This encodes the project's core principle — **evidence, not opinion**. The
LLM is never asked the open-ended question "are these rules in conflict?"
That framing fails: the research in §13 records LLM-only conflict detection
at 0% recall in the one published head-to-head, and the "just ask an LLM"
claim was refuted under verification. Instead it is handed a concrete
transcript and a grounded, closed question — *"given rule A, rule B, and
this run, which did the agent follow: A, B, both reconciled, neither, or did
it ask?"* Classification against evidence, not judgment about text. That is
why the same machinery, harvested from SkillOpt's transport (§8, §12), can
power the tier at zero API cost, and why its verdicts are trustworthy enough
to pin in the lock and gate on.

## 6. Quality pillar: best-practice review

Consistency (§5) checks rules *against each other*. The quality pillar
checks each skill *against how skills should be written* — the ESLint-style
"this is written badly" layer, complementary to the "these contradict"
layer. It needs no fleet: it helps someone with a single skill on day one,
which makes it the natural adoption on-ramp into the consistency features.

### The disclosure ladder

Every extracted rule is tagged with the tier it lives at (§3, Role). This is
the load-bearing primitive for *both* pillars:

| Tier | Loaded | Cost of a wasted/duplicated rule |
|---|---|---|
| description | always | highest — in every context |
| body (`SKILL.md`) | on trigger | medium |
| reference (sibling files) | on demand | low, but easy to orphan |

Tier tells you how *hot* a rule is, which sets (a) conflict severity in the
consistency pillar — two always-on rules clashing is a fire; an always-on
rule vs. a rarely-loaded reference is a smoulder — and (b) the leanness
budget in the quality pillar.

### Checks

**Leanness** — the always-loaded tiers should be small.
`bloated-skill`, `bloated-description`, and padding prose ("in order to
successfully accomplish this…").

**Progressive disclosure** — lean entry point, detail on demand.
`no-progressive-disclosure` (long skill, zero references), `inline-reference`
(reference-grade detail in the body — demote it), `orphan-reference` (a
reference nothing links to — flagged, not silent), `reference-is-really-a-skill`
(a reference with its own triggers — promote it).

**Emphasis discipline** — clarity over volume. `emphasis-overuse` fires on
the *density* of MUST / NEVER / DO NOT / ALL-CAPS / bold, not any single
use. Heavy emphasis usually means patching model failures with volume
instead of instructing clearly, and it dilutes the emphasis that matters.
(Opt-in: heavy negative framing — "don't do X" is weaker than "do Y".)

**Placement / tier-typed duplication** — the same detection engine as the
consistency pillar, but the verdict depends on *where* the copies live:
`cross-tier-duplicate` (a rule in both a skill's body and its reference or
description — the drift trap inside one skill; pick a tier) and
`redundant-with-root` (a skill rule restating an org `CLAUDE.md` rule — drop
it and inherit, or it drifts from policy).

**Hygiene** — `weak-description` (doesn't say *when* to trigger) and
`dead-reference` (link to a missing file).

### Defaults & configuration

Best practices are opinionated and runtime-specific (Anthropic skill norms
≠ Cursor rule norms), so:

- a **curated core** fires by default but is **advisory** — it warns, it
  does not block CI (leanness, orphan/dead references, cross-tier
  duplication, emphasis-overuse);
- **stylistic** checks (negative framing, prose padding) are **opt-in**;
- every check carries a rationale and a link to source guidance, lives in
  the per-format **adapter** (each runtime brings its own norms), and is
  individually suppressible in the lock.

The consistency pillar can gate CI hard; the quality pillar advises. Both
write to the same `tenetic.lock`, so `check` reports across both and
`baseline` accepts across both.

### Prior art

Largely *harvested*, not invented: skillspec's `lint.rs` already ships
`context-too-large` (leanness), `critical-overuse` (the
everything-is-emphasized smell), `unused-lazy-context` (orphan reference),
and `uniform-priority`. And SkillSpec's *lazy context* construct — summary
always-on, detail on demand — is exactly the disclosure ladder, now read
natively from markdown file structure instead of a DSL keyword.

## 7. Remediation: closing the loop (`tenetic fix`)

Detection without remediation is half a tool — `plan` with no `apply`.
Tenetic's `fix` tier turns a confirmed finding into a proposed edit, gates it
through the same detectors that found it, verifies it with a probe, and pins
the resolution. The loop closes: **detect → confirm → resolve → verify → pin.**

### The resolution ladder

Most resolutions are not optimization — they are mechanical, which is what
makes auto-fix safe to ship:

- **Tier A — deterministic (no LLM).** `duplicate`, `cross-tier-duplicate`,
  and `redundant-with-root` → delete the redundant copy (or lift it to a
  shared root); `drifted-duplicate` → present the diff and adopt one
  canonical form; `emphasis-overuse`, `inline-reference`, and the leanness
  findings → mechanical rewrites. Harvested directly from skillspec's
  `lint --fix`.
- **Tier B — proposed edits (native, thin LLM).** For genuine
  contradictions — where you must pick a winner or synthesise a properly
  scoped reconciling rule ("always lint **except** on generated files") — a
  targeted proposer drives the probe transport (Tenetic's own, §8/§12); no
  SkillOpt.
- **Tier C — score-driven optimization (SkillOpt, opt-in).** When a
  resolution must also *not hurt task performance*, SkillOpt is the right
  engine: propose an edit, run it against the skill's `tests {}`, keep it
  only if the score holds. This is where SkillOpt is genuinely baked in —
  behind Tenetic's loop, as an opt-in backend.

Default is Tiers A+B — native, zero-dependency, in the wheel. Tier C is
opt-in, so a Python training engine never gates the one-minute install.

### Propose, then let the engine dispose

Every proposed edit — from any tier — is re-checked before it lands:

```
finding → propose fix (A/B/C) → re-run detectors (no NEW conflict introduced?)
        → probe (does the model now obey the intended rule?) → pin resolution
```

The optimizer proposes; Tenetic's own detectors and probes dispose. This is
the "gate the optimizer" principle turned inward: Tenetic gates its *own*
remediation exactly as it gates SkillOpt-Sleep's nightly writebacks — the
same engine that finds a conflict validates its fix, so an auto-fix cannot
silently make the fleet worse. Resolutions are recorded in `tenetic.lock`,
so a fix later undone re-opens the finding.

### Finding → fix strategy

| Finding | Default strategy | Tier |
|---|---|---|
| `duplicate`, `cross-tier-duplicate`, `redundant-with-root` | delete redundant / lift to root | A |
| `drifted-duplicate` | adopt canonical form | A |
| `emphasis-overuse`, `inline-reference`, `bloated-*` | mechanical rewrite | A |
| `ambiguity`, `unmarked-exception`, `policy-violation` | scoped reconciling edit | B |
| `shadowing`, `ordering-contradiction` | reorder / re-prioritise | B |
| any, when task-score must hold | performance-aware rewrite | C |

## 8. Distribution: uv + maturin (Python-native)

The engine is Rust, but the users who most need fleet governance —
data/ML platform teams — live in Python: Databricks notebooks and jobs,
Airflow/Dagster DAGs, CI runners with a Python toolchain and no Rust. Tenetic
ships as a **native Python extension module** alongside the standalone CLI,
from a single codebase, so `pip install tenetic` (or `uv add tenetic`) gets you
a prebuilt wheel with **no Rust toolchain, no compilation, no network calls
at runtime**.

### Build stack

- **[PyO3](https://pyo3.rs)** — Rust bindings exposing the core API to Python.
- **[maturin](https://www.maturin.rs)** — build backend (`build-system` in
  `pyproject.toml`) that compiles the Rust crate into a Python wheel.
- **[uv](https://docs.astral.sh/uv)** — the dev and install workflow:
  `uv run`, `uv build`, `uvx tenetic` for zero-install CLI use.
- **abi3 (`abi3-py39`)** — build one stable-ABI wheel per platform that
  works on CPython 3.9+, instead of one per Python minor version. Keeps the
  release matrix small.

One repo produces three artifacts:

| Artifact | Consumer | How |
|---|---|---|
| `tenetic` binary | CLI / CI / GitHub Action | `cargo build --release` (or `cargo binstall`) |
| `tenetic` wheel | Python / Databricks / notebooks | `maturin build --release`, published to PyPI |
| `tenetic-core` crate | Rust integrators | `crates.io` |

### `pyproject.toml` (sketch)

```toml
[build-system]
requires = ["maturin>=1.7,<2.0"]
build-backend = "maturin"

[project]
name = "tenetic"
requires-python = ">=3.9"
dynamic = ["version"]

[tool.maturin]
features = ["pyo3/extension-module", "python"]
module-name = "tenetic._native"
# bin target stays available for `cargo install`; the wheel ships the ext module
```

The Python binding is a thin `#[cfg(feature = "python")]` layer over the
same `tenetic-core` functions the CLI calls — no logic forks between the two
front ends.

### Python API surface

The scan/check/probe loops, returned as plain Python objects (dicts /
dataclasses), so they compose with pandas, Delta tables, and notebook
display:

```python
import tenetic

# Scan a skills directory (local path, DBFS, or Unity Catalog volume)
report = tenetic.scan("/Volumes/main/agents/skills")

report.summary()                      # {'duplicate': 3, 'polarity-conflict': 1, ...}
for f in report.findings:
    print(f.kind, f.a.file, f.a.line, "<->", f.b.file, f.b.line)

# CI-style gate against a pinned lock — raises on new findings
tenetic.check("/Volumes/main/agents/skills", lock="tenetic.lock")

# Findings as a DataFrame for dashboards / Delta
import pandas as pd
df = pd.DataFrame(f.as_dict() for f in report.findings)
```

### Databricks usage

- **Install:** `%pip install tenetic` in a notebook, or add to the cluster's
  environment / a `uv`-managed job. Prebuilt manylinux wheel → no build step
  on the cluster.
- **Where it reads:** local paths, DBFS, and Unity Catalog **Volumes** (skill
  documents governed as data). A scheduled **Databricks Job** runs
  `tenetic.check(...)` nightly and on skill-repo changes; findings land in a
  Delta table for lineage and dashboards.
- **Probes:** the behavioral tier can route through a Databricks
  **model-serving endpoint** or Foundation Model API as the target model,
  reusing the same request/response transport (a serving-endpoint transport
  is an adapter, parallel to the hosting-agent one).
- **Governance fit:** rule documents become a governed asset with a semantic
  state file, drift detection, and an audit trail — the story platform teams
  already understand for data, applied to agent instructions.

### Release automation

`maturin-action` in CI builds wheels across manylinux / macOS
(x86_64 + arm64) / Windows plus an sdist, and publishes to PyPI on tag —
mirroring how ruff and pydantic-core ship. The CLI binary and the wheel cut
from the same tag, so versions never skew.

## 9. The best of SkillSpec, carried over without the language

The DSL is retired; its *semantics* survive as optional frontmatter
annotations on plain markdown (progressive hardening — each unlocks
checks, none is required):

| SkillSpec construct | Tenetic form | Unlocks |
|---|---|---|
| `context(priority: critical)` | `priority:` on frontmatter or per-bullet `[!critical]` marker | erosion detection, shadowing analysis, trim-order |
| `context(target: cursor)` | `targets:` frontmatter | scenario scoping |
| `budget { max_tokens }` | `budget:` frontmatter | fleet-serialization budget check |
| `version "1.2.0"` + semver classification | `version:` frontmatter + `tenetic diff` | derived (not declared) semver on rulebooks |
| Structural diff engine | `tenetic diff` | change classification, erosion |
| `build --check` deploy gate | `tenetic check --deployed` | source-vs-deployed drift |
| optimize's hosting-agent LLM transport | probe runner + Tier-B proposer | behavioral tier + native remediation, zero API cost |
| optimize's SkillOpt loop | Tier-C remediation engine | performance-aware conflict resolution, gated & opt-in |
| `lint --fix` mechanical fixes | Tier-A remediation | deterministic conflict/quality auto-fix |
| Token budget estimator | rulebook serialization sizing | lost-in-the-middle positioning warnings |

Explicitly left behind: the grammar, lexer/parser/formatter/LSP ambitions,
compile targets as a product (adapters read formats; they don't own them),
packages/registry, pipelines/orchestrations-as-syntax.

## 10. Positioning

- **One-liner:** *ESLint finds bugs in your code. Tenetic finds them in your
  agents' instructions.*
- **The Terraform claim, made precisely:** Terraform's moat was never HCL —
  it was state + plan + drift detection over a world others own. Tenetic
  makes the same play one level up: the world is the set of rule documents
  across every agent runtime; `tenetic.lock` is the state; `check` is the
  plan-time drift detector; probes extend state from "what the rulebook
  says" to "what the fleet does"; and `tenetic fix` (§7) is `apply` — the
  gated write-side that most drift tools never earn. Format-neutral the way
  Terraform was cloud-neutral.
- **Non-competition:** eval platforms (promptfoo, Braintrust) measure task
  quality; optimizers (SkillOpt/GEPA, DSPy) mutate single documents. Tenetic
  is the static+behavioral consistency layer *between* them. It also
  *drives* SkillOpt as its opt-in Tier-C remediation engine (§7) **and**
  gates its writebacks in the same loop — proposer and safety gate at once,
  so an optimizer can't mint fresh cross-skill conflicts while maximising
  one skill's score.
- **Wedge order:** (1) single dev, one scan, one caught conflict; (2) the
  CI action on the team repo; (3) strata/policy for platform teams
  ("agents may not contradict the org constitution"); (4) probe reports as
  the model-upgrade go/no-go artifact.

## 11. Milestones

- **M0 — Harvest (days).** New repo; port `rules.rs` engine, extraction,
  lock, CLI skeleton, CI action; rename vocabulary to tenets/findings.
  Ships `scan`/`check`/`baseline` at parity with skillspec today.
  Stand up the dual build from day one: `tenetic-core` crate + `tenetic` CLI +
  PyO3/maturin wheel with `scan`/`check` exposed to Python, published to
  PyPI via `maturin-action` (§8). Getting packaging right early is cheaper
  than retrofitting it, and the wheel is what unlocks Databricks pilots.
  Also port `lint.rs` as the seed of the **quality pillar** (§6) with the
  `review` command and tier tagging — harvested and deterministic, and the
  single-skill on-ramp that delivers value before a fleet exists.
- **M1 — Relationships & rulebook (1–2 wk).** Specialization detection,
  `unmarked-exception`, `shadowing`, strata + `policy-violation`,
  `erosion`, effective-rulebook serialization with position warnings.
  This milestone kills most false positives and adds the order dimension.
- **M2 — Graph (1–2 wk).** Reference/artifact/ordering edges;
  `dangling-reference`, `inlined-copy`, `ordering-contradiction`;
  `tenetic graph`.
- **M3 — Probes (2 wk).** Hosting-agent transport, probe generation from
  finding subjects (+ `tests` data when present), verdict schema, verdicts
  in lock, `behavioral-flip` detection. Side product: every probe verdict
  is a labeled example — this *creates* the imperative-rule conflict
  dataset that does not exist in the literature.
- **M4 — Remediation (`tenetic fix`, 1–2 wk).** Tier A (harvested from
  `lint --fix`) and Tier B (native proposer over the probe transport), each
  gated by re-detection + a probe before pinning. Tier C (opt-in SkillOpt
  backend) lands last. After probes deliberately: confirm a conflict before
  auto-resolving it.
- **M5 — Semantic tier (gated).** Embedding pairing + local NLI behind
  `--semantic`, shipped **only if** it beats tiers 0–2 recall on the
  M3-generated labeled set by a margin worth the model download.
- **M6 — Fleet.** Multi-repo state, org dashboards, model-upgrade probe
  reports, policy packs.

## 12. Harvest map (from skillspec repo)

| Take | From | Becomes |
|---|---|---|
| `src/rules.rs` (engine, lock, tests) | skillspec | `tenetic-core` (rename Rule→Tenet) |
| `src/migrate.rs` markdown parsing | skillspec | format adapters |
| `src/diff.rs` structural diff + semver | skillspec | `tenetic diff` |
| `src/budget.rs` estimator | skillspec | serialization sizing |
| `src/lint.rs` rules + `lint --fix` | skillspec | quality-pillar checks (§6) + Tier-A deterministic remediation (§7) |
| `optimize.rs` request/response protocol | skillspec | probe transport + Tier-B remediation proposer (§7) |
| `optimize.rs` SkillOpt setup/loop integration | skillspec | Tier-C remediation engine — opt-in, gated by Tenetic (§7) |
| `docs/research-conflict-detection.md` | skillspec | design bibliography |
| `action.yml` pattern, CI workflow | skillspec | `tenetic-action` |
| `.agent` parser | skillspec | one adapter among several (maintenance mode) |
| Rust workspace layout, single-binary discipline | skillspec | `tenetic-core` lib + `tenet` bin + PyO3 wheel from one tree (§8) |

## 13. Evidence base

Design decisions trace to an adversarially verified literature survey
(`docs/research-conflict-detection.md` in the skillspec repo): FSARC
(deterministic detection, ~100% recall / 84% precision → suppression UX),
de Marneffe et al. 2008 (scope filtering is mandatory; easy/hard
contradiction typology → tier boundaries), S3CDA (embedding pairing; local
beats zero-shot GPT-4o on domain text), DECODE (12–16pt NLI domain-transfer
gap → benchmark before shipping tier 3; pairwise beats whole-document),
ALICE (hybrid 60% vs LLM-only 0% recall → probes ask narrow questions of
traces, never open-ended judgment).

## 14. Risks & open questions

- **Extraction recall on messy prose.** Diffuse, paragraph-level
  instructions resist atomic extraction. Mitigation: over-extract +
  suppress; measure against real fleets early.
- **Scenario inference.** Trigger/scope semantics differ per runtime
  (Claude skills trigger by description; Cursor rules by glob). Adapters
  must encode each runtime's co-activation model; wrong scenarios create
  false conflicts. Start conservative (assume co-active), refine per
  adapter.
- **Probe realism.** Synthetic probes may not match real task
  distributions. Prefer `tests`-derived inputs; report verdicts with
  confidence, not certainty.
- **Name clearance.** "Tenetic" is free on crates.io / npm / PyPI, but a
  funded media-analytics firm (tenetic.com, launched 2025) uses the bare
  word. Different industry lowers confusion risk; overlap is the software/
  SaaS class. Mitigations before public launch: trademark-class search,
  distinct domain/handles (tenetic.dev, teneticHQ), and consistent
  "Tenetic for agent rulebooks" framing. Registry-clear fallbacks if it
  fails clearance: Ruleward, Canonry.
- **Platform absorption.** Anthropic/OpenAI could ship native skill
  linting. Defense: cross-runtime neutrality and the lock/probe state
  model — the same defense Terraform ran against CloudFormation.
- **Auto-fix changing intent.** A proposed edit can clear a conflict yet
  drift the meaning. Mitigation: every fix is re-checked by the detectors
  and a probe before it lands, defaults to a diff you approve (not a silent
  apply), and is recorded so it can be reverted; Tier A (mechanical) applies
  more freely than Tiers B/C.
- **Packaging matrix cost.** Native wheels mean a build matrix (manylinux,
  macOS x86_64/arm64, Windows) and the usual glibc-version footguns.
  Mitigation: abi3 single-wheel-per-platform, `maturin-action`'s prebuilt
  containers, and pure-Rust core deps (no C/OpenSSL linkage) so manylinux
  stays clean. Tiers 0–2 have zero native deps; tier-3 ONNX models are
  downloaded at first use, not baked into the wheel.

## 15. Success criteria

- A cold `tenetic scan` on a real >20-skill fleet surfaces ≥1 finding the
  owner confirms as real, in <5 s, with ≥50% confirmed-useful rate.
- The origin-story test: replaying last-weekend's debugging session against
  Tenetic catches the conflicts in minutes, and `check` provably blocks their
  reintroduction.
- M3 produces ≥200 labeled probe verdicts — the first public benchmark for
  imperative-rule conflict detection.
- `%pip install tenetic` in a fresh Databricks notebook to a working
  `tenetic.scan(...)` in under a minute, no cluster build step — the wheel is
  prebuilt and dependency-light.
