# Tenet — product specification

**Tenet is the consistency and drift control plane for AI agent rulebooks.**
It extracts every rule your agents follow — from SKILL.md files, CLAUDE.md,
Cursor rules, AGENTS.md, system prompts — detects conflicts, drift, and
shadowing across the whole fleet, pins the accepted state in a lockfile, and
probes how models actually adjudicate the tensions.

*A tenet is a rule you hold. Tenet makes sure your agents hold the same ones.*

> Status: specification. Seeds a new repository; harvests specific modules
> from [skillspec](https://github.com/e01n0/skillspec) (see §10). Naming
> pending trademark search; alternates: Canon, Maxim, Concord.

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
research survey (§11) found no paper, benchmark, or product that
consistency-checks agent rule collections.

## 2. Product thesis

1. **No new language, no migration.** Markdown stays the source of truth.
   Tenet reads what teams already have. Adoption cost of the first scan:
   zero. (Decision record: Tenet's predecessor, SkillSpec, was a typed DSL
   compiling to SKILL.md. The operational loops turned out to be the value;
   the language was the toll booth. Tenet inverts that: loops first,
   optional structure later — carried as frontmatter annotations, never as
   a required syntax.)
2. **Deterministic first.** The core engine uses no models and no network:
   reproducible in CI, millisecond-fast, zero API cost. Learned tiers
   (embeddings, local NLI) are opt-in and local; LLM judgment appears in
   exactly one place — behavioral probes — where the trace is evidence, not
   opinion. Grounded in verified literature (§11): local pipelines beat
   zero-shot GPT-4o on domain rule text; LLM-only conflict detection scored
   0% recall in the one published head-to-head.
3. **State, not snapshots.** `tenet.lock` is the Terraform move: a pinned
   record of the fleet's semantic state (known findings, accepted
   exceptions, probe verdicts). CI fails only on *changes* to that state,
   so fixed conflicts stay fixed and triaged noise stays triaged.

## 3. Core concepts

| Concept | Definition |
|---|---|
| **Tenet** | An atomic imperative rule extracted from a source document, with provenance (file, line, owning skill), polarity, subject keywords, and — where declared — priority, scope, and version. |
| **Source** | Any document contributing tenets: SKILL.md, CLAUDE.md / AGENTS.md, `.cursorrules`/`.mdc`, system-prompt files, SkillSpec `.agent` (supported via adapter). |
| **Stratum** | The precedence level of a source. Root documents (org policy, CLAUDE.md) outrank skills; a skill tenet contradicting a root tenet is a *policy violation*, not a peer conflict. |
| **Scenario** | A co-activation set: the sources that can be loaded into one context simultaneously (derived from triggers, targets, and path scopes). Tenets that never co-activate cannot conflict. |
| **Rulebook** | The *effective rulebook* of a scenario: the serialized, ordered, precedence-resolved sequence of tenets the model actually sees. The unit of analysis for ordering checks. |
| **Finding** | A classified relationship between tenets (taxonomy below) with a stable content-hash identity that survives line moves and reformatting. |
| **Lock** | `tenet.lock`: accepted findings, tenet inventory fingerprint, probe verdicts. The semantic state file. |
| **Probe** | A behavioral experiment: compose the scenario's rulebook, pose a task that forces a flagged tension, run N times, judge which tenet the model obeyed. |

### Finding taxonomy

Adapted from firewall-policy conflict analysis (shadowing / generalization /
correlation / redundancy) plus the drift classes:

| Kind | Meaning | Tier |
|---|---|---|
| `duplicate` | Identical tenet in multiple places | 0 |
| `drifted-duplicate` | Near-identical copies that diverged | 0 |
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

## 4. Product surface

```sh
tenet scan  [path]              # extract tenets + report findings (read-only)
tenet check [path]              # CI gate: fail on findings not in tenet.lock
tenet baseline [path]           # pin current findings as accepted state
tenet graph [path] --format dot|mermaid   # reference/artifact/ordering graph
tenet probe [finding-id|--new]  # behavioral probes via hosting agent session
tenet diff <old> <new>          # semantic diff of two fleet states; semver class
tenet explain <finding-id>      # full provenance, both tenets in context, why flagged
```

- **CI:** a published GitHub Action (`uses: <org>/tenet@v1`) running
  `check` on every PR touching rule files. This is the product's primary
  distribution channel — "the failing check that saved your weekend" is the
  growth loop.
- **Watch mode:** `tenet check --watch` for local editing sessions.
- **Output contracts:** human-readable report; `--json` for editor/agent
  integration (agents editing skills should run Tenet themselves —
  a Claude Code hook recipe ships in the docs).

### Suppression & triage UX

The literature's validated precision ceiling for deterministic detection is
~84%. Therefore, first-class:
- `baseline` accepts everything current (adopt-in-anger path);
- per-finding accept with a reason (`tenet accept <id> --reason "…"`),
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
   │                 filtering, polarity, keyword normalization
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
tenet.lock  ←→  check / diff / report
```

Implementation: Rust core (harvested — §10), shipped as both a single
static binary and a PyO3/maturin Python wheel (§6) from one codebase. No
runtime dependencies for tiers 0–2. Tier 3 models load lazily behind a
flag. Probes require only a hosting agent session (Claude Code, Cursor) or
a model-serving endpoint, zero API keys, via the checkpoint-resume
request/response protocol.

## 6. Distribution: uv + maturin (Python-native)

The engine is Rust, but the users who most need fleet governance —
data/ML platform teams — live in Python: Databricks notebooks and jobs,
Airflow/Dagster DAGs, CI runners with a Python toolchain and no Rust. Tenet
ships as a **native Python extension module** alongside the standalone CLI,
from a single codebase, so `pip install tenet` (or `uv add tenet`) gets you
a prebuilt wheel with **no Rust toolchain, no compilation, no network calls
at runtime**.

### Build stack

- **[PyO3](https://pyo3.rs)** — Rust bindings exposing the core API to Python.
- **[maturin](https://www.maturin.rs)** — build backend (`build-system` in
  `pyproject.toml`) that compiles the Rust crate into a Python wheel.
- **[uv](https://docs.astral.sh/uv)** — the dev and install workflow:
  `uv run`, `uv build`, `uvx tenet` for zero-install CLI use.
- **abi3 (`abi3-py39`)** — build one stable-ABI wheel per platform that
  works on CPython 3.9+, instead of one per Python minor version. Keeps the
  release matrix small.

One repo produces three artifacts:

| Artifact | Consumer | How |
|---|---|---|
| `tenet` binary | CLI / CI / GitHub Action | `cargo build --release` (or `cargo binstall`) |
| `tenet` wheel | Python / Databricks / notebooks | `maturin build --release`, published to PyPI |
| `tenet-core` crate | Rust integrators | `crates.io` |

### `pyproject.toml` (sketch)

```toml
[build-system]
requires = ["maturin>=1.7,<2.0"]
build-backend = "maturin"

[project]
name = "tenet"
requires-python = ">=3.9"
dynamic = ["version"]

[tool.maturin]
features = ["pyo3/extension-module", "python"]
module-name = "tenet._native"
# bin target stays available for `cargo install`; the wheel ships the ext module
```

The Python binding is a thin `#[cfg(feature = "python")]` layer over the
same `tenet-core` functions the CLI calls — no logic forks between the two
front ends.

### Python API surface

The scan/check/probe loops, returned as plain Python objects (dicts /
dataclasses), so they compose with pandas, Delta tables, and notebook
display:

```python
import tenet

# Scan a skills directory (local path, DBFS, or Unity Catalog volume)
report = tenet.scan("/Volumes/main/agents/skills")

report.summary()                      # {'duplicate': 3, 'polarity-conflict': 1, ...}
for f in report.findings:
    print(f.kind, f.a.file, f.a.line, "<->", f.b.file, f.b.line)

# CI-style gate against a pinned lock — raises on new findings
tenet.check("/Volumes/main/agents/skills", lock="tenet.lock")

# Findings as a DataFrame for dashboards / Delta
import pandas as pd
df = pd.DataFrame(f.as_dict() for f in report.findings)
```

### Databricks usage

- **Install:** `%pip install tenet` in a notebook, or add to the cluster's
  environment / a `uv`-managed job. Prebuilt manylinux wheel → no build step
  on the cluster.
- **Where it reads:** local paths, DBFS, and Unity Catalog **Volumes** (skill
  documents governed as data). A scheduled **Databricks Job** runs
  `tenet.check(...)` nightly and on skill-repo changes; findings land in a
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

## 7. The best of SkillSpec, carried over without the language

The DSL is retired; its *semantics* survive as optional frontmatter
annotations on plain markdown (progressive hardening — each unlocks
checks, none is required):

| SkillSpec construct | Tenet form | Unlocks |
|---|---|---|
| `context(priority: critical)` | `priority:` on frontmatter or per-bullet `[!critical]` marker | erosion detection, shadowing analysis, trim-order |
| `context(target: cursor)` | `targets:` frontmatter | scenario scoping |
| `budget { max_tokens }` | `budget:` frontmatter | fleet-serialization budget check |
| `version "1.2.0"` + semver classification | `version:` frontmatter + `tenet diff` | derived (not declared) semver on rulebooks |
| Structural diff engine | `tenet diff` | change classification, erosion |
| `build --check` deploy gate | `tenet check --deployed` | source-vs-deployed drift |
| optimize's hosting-agent LLM transport | probe runner | behavioral tier with zero API cost |
| Token budget estimator | rulebook serialization sizing | lost-in-the-middle positioning warnings |

Explicitly left behind: the grammar, lexer/parser/formatter/LSP ambitions,
compile targets as a product (adapters read formats; they don't own them),
packages/registry, pipelines/orchestrations-as-syntax.

## 8. Positioning

- **One-liner:** *ESLint finds bugs in your code. Tenet finds them in your
  agents' instructions.*
- **The Terraform claim, made precisely:** Terraform's moat was never HCL —
  it was state + plan + drift detection over a world others own. Tenet
  makes the same play one level up: the world is the set of rule documents
  across every agent runtime; `tenet.lock` is the state; `check` is the
  plan-time drift detector; probes extend state from "what the rulebook
  says" to "what the fleet does". Format-neutral the way Terraform was
  cloud-neutral.
- **Non-competition:** eval platforms (promptfoo, Braintrust) measure task
  quality; optimizers (SkillOpt/GEPA, DSPy) mutate single documents. Tenet
  is the static+behavioral consistency layer *between* them — and the
  safety gate on optimizers' writebacks, which can otherwise mint fresh
  cross-skill conflicts while maximizing one skill's score.
- **Wedge order:** (1) single dev, one scan, one caught conflict; (2) the
  CI action on the team repo; (3) strata/policy for platform teams
  ("agents may not contradict the org constitution"); (4) probe reports as
  the model-upgrade go/no-go artifact.

## 9. Milestones

- **M0 — Harvest (days).** New repo; port `rules.rs` engine, extraction,
  lock, CLI skeleton, CI action; rename vocabulary to tenets/findings.
  Ships `scan`/`check`/`baseline` at parity with skillspec today.
  Stand up the dual build from day one: `tenet-core` crate + `tenet` CLI +
  PyO3/maturin wheel with `scan`/`check` exposed to Python, published to
  PyPI via `maturin-action` (§6). Getting packaging right early is cheaper
  than retrofitting it, and the wheel is what unlocks Databricks pilots.
- **M1 — Relationships & rulebook (1–2 wk).** Specialization detection,
  `unmarked-exception`, `shadowing`, strata + `policy-violation`,
  `erosion`, effective-rulebook serialization with position warnings.
  This milestone kills most false positives and adds the order dimension.
- **M2 — Graph (1–2 wk).** Reference/artifact/ordering edges;
  `dangling-reference`, `inlined-copy`, `ordering-contradiction`;
  `tenet graph`.
- **M3 — Probes (2 wk).** Hosting-agent transport, probe generation from
  finding subjects (+ `tests` data when present), verdict schema, verdicts
  in lock, `behavioral-flip` detection. Side product: every probe verdict
  is a labeled example — this *creates* the imperative-rule conflict
  dataset that does not exist in the literature.
- **M4 — Semantic tier (gated).** Embedding pairing + local NLI behind
  `--semantic`, shipped **only if** it beats tiers 0–2 recall on the
  M3-generated labeled set by a margin worth the model download.
- **M5 — Fleet.** Multi-repo state, org dashboards, model-upgrade probe
  reports, policy packs.

## 10. Harvest map (from skillspec repo)

| Take | From | Becomes |
|---|---|---|
| `src/rules.rs` (engine, lock, tests) | skillspec | `tenet-core` (rename Rule→Tenet) |
| `src/migrate.rs` markdown parsing | skillspec | format adapters |
| `src/diff.rs` structural diff + semver | skillspec | `tenet diff` |
| `src/budget.rs` estimator | skillspec | serialization sizing |
| `optimize.rs` request/response protocol | skillspec | probe transport |
| `docs/research-conflict-detection.md` | skillspec | design bibliography |
| `action.yml` pattern, CI workflow | skillspec | `tenet-action` |
| `.agent` parser | skillspec | one adapter among several (maintenance mode) |
| Rust workspace layout, single-binary discipline | skillspec | `tenet-core` lib + `tenet` bin + PyO3 wheel from one tree (§6) |

## 11. Evidence base

Design decisions trace to an adversarially verified literature survey
(`docs/research-conflict-detection.md` in the skillspec repo): FSARC
(deterministic detection, ~100% recall / 84% precision → suppression UX),
de Marneffe et al. 2008 (scope filtering is mandatory; easy/hard
contradiction typology → tier boundaries), S3CDA (embedding pairing; local
beats zero-shot GPT-4o on domain text), DECODE (12–16pt NLI domain-transfer
gap → benchmark before shipping tier 3; pairwise beats whole-document),
ALICE (hybrid 60% vs LLM-only 0% recall → probes ask narrow questions of
traces, never open-ended judgment).

## 12. Risks & open questions

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
- **Name clearance.** Tenet vs. existing marks (film, healthcare co).
  Fallbacks: Canon, Maxim, Concord.
- **Platform absorption.** Anthropic/OpenAI could ship native skill
  linting. Defense: cross-runtime neutrality and the lock/probe state
  model — the same defense Terraform ran against CloudFormation.
- **Packaging matrix cost.** Native wheels mean a build matrix (manylinux,
  macOS x86_64/arm64, Windows) and the usual glibc-version footguns.
  Mitigation: abi3 single-wheel-per-platform, `maturin-action`'s prebuilt
  containers, and pure-Rust core deps (no C/OpenSSL linkage) so manylinux
  stays clean. Tiers 0–2 have zero native deps; tier-3 ONNX models are
  downloaded at first use, not baked into the wheel.

## 13. Success criteria

- A cold `tenet scan` on a real >20-skill fleet surfaces ≥1 finding the
  owner confirms as real, in <5 s, with ≥50% confirmed-useful rate.
- The origin-story test: replaying last-weekend's debugging session against
  Tenet catches the conflicts in minutes, and `check` provably blocks their
  reintroduction.
- M3 produces ≥200 labeled probe verdicts — the first public benchmark for
  imperative-rule conflict detection.
- `%pip install tenet` in a fresh Databricks notebook to a working
  `tenet.scan(...)` in under a minute, no cluster build step — the wheel is
  prebuilt and dependency-light.
