# Prior art: detecting conflicts and drift across natural-language rule sets

Research survey informing `skillspec rules` — the cross-skill conflict linter.
Question: what is known about automatically detecting contradictions,
conflicts, and drift across sets of natural-language rules or instructions?

Method: multi-angle web research (requirements engineering, NLI/contradiction
detection, formal policy conflict, LLM-era instruction conflicts, existing
tooling); 22 sources fetched, 25 top claims adversarially verified by
independent 3-vote panels; 24 survived, 1 refuted. Conducted 2026-07.

---

## Headline conclusions

1. **Every stage of a deterministic-first pipeline has published validation**
   (dedup → scope-match → parse-based polarity clash → embedding pairing →
   local NLI → optional targeted LLM). Nothing needs to be invented from
   scratch; the composition for agent skill files is what's new.
2. **LLM-only conflict detection is the worst measured architecture**, and
   local deterministic/embedding pipelines beat zero-shot frontier LLMs on
   domain-specific rule text.
3. **No paper, benchmark, or tool consistency-checks agent skill files**
   (SKILL.md collections, system prompts, Cursor rules) across documents.
   The territory is unoccupied.

## Verified findings by pipeline stage

### Deterministic extraction + rule-based conflict checks (Tier 1)

- **FSARC** ([arXiv:2103.02255](https://arxiv.org/pdf/2103.02255)) maps each
  requirement to an eight-tuple `{id, groupId, event, agent, operation,
  input, output, restriction}` via CoreNLP POS + dependency parsing, then
  runs heuristic checks for seven conflict types (inconsistency, inclusion,
  interlock). Fully deterministic — no ML in the detection stage.
  **Reported: ~100% recall, 83.9% average precision.** Design lesson: expect
  roughly 1 in 6 flags to be noise → a suppression/allow-list mechanism is a
  requirement, not a nicety.
- **QuARS** (canonical deterministic requirements linter) is per-sentence
  quality linting only; its authors explicitly place cross-rule conflict
  detection outside its capability. Confirms the per-file/per-sentence
  linting niche is solved and the *pairwise* problem is the open one.

### Scope matching before conflict features (co-activation)

- **Finding Contradictions in Text** (de Marneffe, Rafferty & Manning,
  [ACL 2008](https://aclanthology.org/P08-1118/)) — the foundational paper —
  found event-coreference filtering is *mandatory* before mismatch features:
  contrasting statements about different events otherwise produce false
  contradictions. For a rule linter this maps directly to scope matching
  (same tool / file type / trigger conditions) before flagging polarity
  clashes. This is not an optimization; it is load-bearing.
- Same paper's typology drives tier placement: **antonymy, negation, and
  numeric mismatch are detectable deterministically** with closed-class
  resources; factive/modal/structural/world-knowledge contradictions are
  not (their lexical-contradiction recall on held-out data: 0%).

### Embedding candidate pairing (Tier 2)

- **S3CDA** ([arXiv:2206.13690](https://arxiv.org/pdf/2206.13690)):
  cosine-similarity candidate pairing (TF-IDF/USE/SBERT) + entity-overlap
  validation. Macro-F1 up to **0.923**. Notably, plain TF-IDF beat semantic
  embeddings on high-lexical-overlap corpora — similarity choice should be
  benchmarked, not assumed.
- **The local pipeline beat zero-shot GPT-4o on 3 of 5 datasets**
  (0.923 vs 0.462 on UAV). Frontier LLMs won only on general-language text.

### Local NLI cross-encoder (Tier 3)

- Drop-in models exist ([cross-encoder/nli-deberta-v3-base](https://huggingface.co/cross-encoder/nli-deberta-v3-base),
  92.4% SNLI-test) and MNLI-pretraining is the single highest-leverage
  ingredient for learned conflict detection — MNLI-pretrained checkpoints
  fine-tuned on requirements reach **84–99% macro-F1** while non-NLI
  baselines collapse to ~49% ([arXiv:2301.03709](https://arxiv.org/pdf/2301.03709)).
- **But domain transfer is the risk**: DECODE
  ([arXiv:2012.13391](https://arxiv.org/abs/2012.13391)) measured a
  **12–16 point accuracy drop** applying generic NLI training to a new
  contradiction domain. Off-the-shelf NLI accuracy on *imperative* rule
  pairs is unmeasured anywhere. → Build a small labeled eval set from real
  skill conflicts before trusting this tier. If it underperforms,
  [arXiv:2310.14732](https://arxiv.org/pdf/2310.14732) gives a deterministic
  recipe (WordNet antonym substitution, negation insertion, numeric
  perturbation) for synthesizing domain-matched fine-tuning data.
- DECODE also showed **structured pairwise comparison beats whole-document
  classification** out-of-distribution (84.7% vs 70.0%) — extracting atomic
  rules and comparing pairs is the right unit of analysis.

### Learned recall + deterministic precision (hybrid ordering)

- Semantic-role-labeling post-filters on a transformer's conflict
  predictions **eliminated up to 99.5% of false positives** (+0.23–0.39
  absolute macro-F1) ([arXiv:2301.03709](https://arxiv.org/pdf/2301.03709)).
  Caveat: the filter also discarded 20–56% of true positives — filters
  trade recall for precision and should be tunable.

### Optional LLM tier: targeted questions only (Tier 4)

- **ALICE** ([Autom. Softw. Eng. 2024](https://link.springer.com/article/10.1007/s10515-024-00452-x)):
  deterministic preprocessing feeding an LLM *targeted questions* inside a
  seven-question decision tree found **60% of contradictions on the
  1,071-pair dataset where the LLM-only baseline found 0%** (99% vs 97%
  accuracy). If an LLM tier is ever added, it is ALICE-shaped: narrow
  questions about pre-parsed pairs, never "find conflicts in these files".
- A claim that ChatGPT catches requirement inconsistencies that NLP tools
  miss was **refuted 0–3** in verification. The "just use an LLM" intuition
  did not survive.

## Unverified leads (surfaced but not adjudicated this run)

- **Firewall/policy conflict taxonomy** (Al-Shaer & Hamed, IEEE JSAC 2005):
  shadowing / generalization / correlation / redundancy — vocabulary worth
  adopting for precedence-aware conflicts between co-triggering skills.
- **Instruction hierarchy** (Wallace et al., OpenAI,
  [arXiv:2404.13208](https://arxiv.org/abs/2404.13208)): about model
  *behavior* under conflicting instructions, not conflict *detection* —
  adjacent, not overlapping.
- **PromptLint**: claims deterministic conflict flagging for single prompts;
  no evidence of cross-file/fleet scope.

## Implications for `skillspec rules`

| Design choice | Basis |
|---|---|
| Rule = atomic imperative sentence with provenance | DECODE: pairwise beats whole-document |
| Scope/co-activation filter before conflict checks | de Marneffe 2008: event coreference is mandatory |
| Deterministic tier limited to dedup, polarity clash, priority mismatch | de Marneffe typology: easy types only |
| Allow-list / baseline suppression in the lockfile | FSARC: ~84% precision at ~100% recall |
| Benchmark any NLI stage on a hand-labeled skill-conflict set first | DECODE: 12–16pt domain-transfer gap |
| LLM tier off by default; targeted-question form if added | ALICE: hybrid 60% vs LLM-only 0% recall; S3CDA beats zero-shot GPT-4o |
