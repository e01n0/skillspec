use crate::token::Span;
use std::collections::HashSet;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SourceFile {
    pub imports: Vec<Import>,
    pub type_defs: Vec<TypeDef>,
    pub skills: Vec<Skill>,
    pub pipelines: Vec<Pipeline>,
    pub orchestrations: Vec<Orchestration>,
    pub mixins: Vec<Mixin>,
    pub packages: Vec<Package>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Package {
    pub name: String,
    pub version: String,
    pub description: String,
    pub exports: Vec<String>,
    pub span: Span,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Import {
    pub symbols: Vec<String>,
    pub path: String,
    pub span: Span,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TypeDef {
    pub name: String,
    pub fields: Vec<Field>,
    pub span: Span,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Field {
    pub name: String,
    pub ty: TypeExpr,
    pub optional: bool,
    pub default: Option<Expr>,
    pub span: Span,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum TypeExpr {
    String,
    Int,
    Float,
    Bool,
    Array(Box<TypeExpr>),
    Map(Box<TypeExpr>, Box<TypeExpr>),
    Enum(Vec<String>),
    Named(String),
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Skill {
    pub name: String,
    pub extends: Option<String>,
    pub input: Option<Vec<Field>>,
    pub output: Option<Vec<Field>>,
    pub pre: Vec<Assertion>,
    pub post: Vec<Assertion>,
    pub body: Body,
    pub span: Span,
    pub tools: Option<ToolsBlock>,
    pub permissions: Option<PermissionsBlock>,
    pub includes: Vec<String>,
    pub tests: Vec<TestBlock>,
}

#[derive(Debug, Clone)]
pub enum BodyItemRef {
    Context(usize),
    LazyContext(usize),
    Step(usize),
    Observe,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Body {
    pub contexts: Vec<ContextBlock>,
    pub lazy_contexts: Vec<LazyContext>,
    pub steps: Vec<Step>,
    pub on_error: Option<Vec<UseCall>>,
    pub directives: PromptDirectives,
    pub observe: Option<ObserveBlock>,
    // Indices valid only for this body's own vecs; not meaningful after extends/mixin merging.
    #[serde(skip)]
    pub source_order: Vec<BodyItemRef>,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub enum Priority {
    Optional,
    Supplementary,
    Important,
    Critical,
}

impl Priority {
    pub fn rank(self) -> u8 {
        match self {
            Priority::Optional => 10,
            Priority::Supplementary => 40,
            Priority::Important => 75,
            Priority::Critical => 100,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Priority::Optional => "optional",
            Priority::Supplementary => "supplementary",
            Priority::Important => "important",
            Priority::Critical => "critical",
        }
    }
}

impl std::fmt::Display for Priority {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ContextBlock {
    pub priority: Option<Priority>,
    pub when: Option<Expr>,
    pub decay: Option<f64>,
    pub until: Option<String>,
    pub text: String,
    pub span: Span,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Step {
    pub name: String,
    pub requires: Option<Dependency>,
    pub when: Option<Expr>,
    pub use_call: Option<UseCall>,
    pub lets: Vec<LetBinding>,
    pub emit: bool,
    pub contexts: Vec<ContextBlock>,
    pub span: Span,
    pub loads: Vec<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum Dependency {
    Single(String),
    All(Vec<String>),
    Any(Vec<String>),
    AllSteps,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UseCall {
    pub skill_name: String,
    pub args: Vec<(String, Expr)>,
    pub span: Span,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LetBinding {
    pub name: String,
    pub value: Expr,
    pub span: Span,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Assertion {
    pub condition: Expr,
    pub when_guard: Option<Expr>,
    pub message: String,
    pub span: Span,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum Expr {
    StringLit(String),
    IntLit(i64),
    FloatLit(f64),
    BoolLit(bool),
    Ident(String),
    FieldAccess(Box<Expr>, String),
    ArrayLit(Vec<Expr>),
    BinOp(Box<Expr>, BinOp, Box<Expr>),
    Not(Box<Expr>),
    FnCall(String, Vec<(String, Expr)>),
    Interpolated(String),
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum BinOp {
    Eq,
    NotEq,
    Lt,
    Gt,
    LtEq,
    GtEq,
    In,
    And,
    Or,
}

// ── Lazy context ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LazyContext {
    pub name: String,
    pub priority: Option<Priority>,
    pub summary: String,
    pub content: LazyContent,
    pub span: Span,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum LazyContent {
    Inline(String),
    Ref(String),
    Index(Vec<IndexSection>),
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct IndexSection {
    pub name: String,
    pub summary: String,
    pub ref_path: String,
}

// ── Tools ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ToolsBlock {
    pub required: Vec<ToolDecl>,
    pub optional: Vec<ToolDecl>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ToolDecl {
    pub kind: ToolKind,
    pub name: String,
    pub methods: Vec<ToolMethod>,
    pub allow: Vec<String>,
    pub deny: Vec<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum ToolKind {
    Builtin,
    Mcp(String),
    Generic,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ToolMethod {
    pub name: String,
    pub params: Vec<(String, TypeExpr, bool)>, // name, type, optional
    pub return_type: TypeExpr,
}

// ── Permissions ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PermissionsBlock {
    pub filesystem: Option<(String, Vec<String>)>, // mode, patterns
    pub network: Option<(String, Vec<String>)>,    // mode, hosts
    pub secrets: Vec<String>,
}

// ── Prompt directives ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct PromptDirectives {
    pub reasoning: Option<String>, // "none", "standard", "extended"
    pub examples: Vec<PromptExample>,
    pub format: Option<FormatDirective>,
    pub reinforcements: Vec<Reinforcement>,
    pub sampling: Option<SamplingDirective>,
    pub persona: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PromptExample {
    pub name: String,
    pub input: String,
    pub output: String,
    pub note: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FormatDirective {
    pub style: String,
    pub structure: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Reinforcement {
    pub trigger: ReinforceTrigger,
    pub text: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum ReinforceTrigger {
    EveryNSteps(i64),
    WhenCondition(Expr),
    OnContextShift,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SamplingDirective {
    pub temperature: Option<f64>,
    pub top_p: Option<f64>,
}

// ── Pipeline ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Pipeline {
    pub name: String,
    pub input: Option<Vec<Field>>,
    pub output: Option<Vec<Field>>,
    pub stages: Vec<PipelineStage>,
    pub on_error: Option<Vec<UseCall>>,
    pub timeout: Option<String>,
    pub span: Span,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PipelineStage {
    pub name: String,
    pub requires: Option<Dependency>,
    pub use_call: UseCall,
    pub span: Span,
}

// ── Orchestration ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Orchestration {
    pub name: String,
    pub agents: Vec<AgentDecl>,
    pub input: Option<Vec<Field>>,
    pub output: Option<Vec<Field>>,
    pub phases: Vec<OrchestratePhase>,
    pub shared: Option<SharedState>,
    pub rules: Vec<OrchestrateRule>,
    pub timeout: Option<String>,
    pub span: Span,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AgentDecl {
    pub name: String,
    pub skill: String,
    pub model: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct OrchestratePhase {
    pub name: String,
    pub requires: Option<Dependency>,
    pub actions: Vec<AgentAction>,
    pub emit: Option<String>, // "from agent.result"
    pub span: Span,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AgentAction {
    pub agent_name: String,
    pub method: String,
    pub args: Vec<(String, Expr)>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SharedState {
    pub fields: Vec<Field>,
    pub handlers: Vec<EventHandler>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EventHandler {
    pub event_source: String,
    pub event_name: String,
    pub body: Vec<Expr>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct OrchestrateRule {
    pub condition: Expr,
    pub actions: Vec<Expr>,
}

// ── Test blocks ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TestBlock {
    pub name: String,
    pub given: Vec<(String, Expr)>,
    pub mocks: Vec<MockDecl>,
    pub expectations: Vec<Expectation>,
    pub confidence: Option<f64>,
    pub runs: Option<i64>,
    pub snapshot: Option<String>,
    pub span: Span,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MockDecl {
    pub tool_path: String,
    pub mock_type: MockType,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum MockType {
    Responses(Vec<MockResponse>),
    Unavailable,
    Failing(String),
    Slow(String),
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MockResponse {
    pub method: String,
    pub args: Vec<(String, Expr)>,
    pub response: Expr,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Expectation {
    pub path: String,
    pub assertion: AssertionExpr,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum AssertionExpr {
    Equals(Expr),
    Contains(Expr),
    Matches(String),
    Resembles(String),
    Satisfies(String),
    Between(Expr, Expr),
    Comparison(BinOp, Expr),
    ContainsWhere(Expr),
    AllWhere(Expr),
    NoneWhere(Expr),
}

// ── Observability ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ObserveBlock {
    pub events: Vec<ObserveEvent>,
    pub metrics: Vec<ObserveMetric>,
    pub span: Span,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ObserveEvent {
    pub trigger: String,
    pub event_name: String,
    pub span: Span,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ObserveMetric {
    pub name: String,
    pub source: Expr,
    pub span: Span,
}

// ── Mixin ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Mixin {
    pub name: String,
    pub steps: Vec<Step>,
    pub contexts: Vec<ContextBlock>,
    pub span: Span,
}

/// Walk the extends chain for a skill, returning ancestors base-first.
/// Stops on cycle or missing base (both handled elsewhere by the checker).
pub fn resolve_ancestry<'a>(skill: &'a Skill, all_skills: &'a [Skill]) -> Vec<&'a Skill> {
    let mut chain = Vec::new();
    let mut seen = HashSet::new();
    seen.insert(&skill.name);
    let mut current = skill.extends.as_ref();
    while let Some(name) = current {
        if !seen.insert(name) {
            break;
        }
        match all_skills.iter().find(|s| &s.name == name) {
            Some(base) => {
                chain.push(base);
                current = base.extends.as_ref();
            }
            None => break,
        }
    }
    chain.reverse();
    chain
}
