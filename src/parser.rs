use crate::ast::*;
use crate::error::{SkillSpecError, Result};
use crate::token::{Span, Token, TokenKind};

pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Parser { tokens, pos: 0 }
    }

    pub fn parse(&mut self) -> Result<SourceFile> {
        let mut imports = Vec::new();
        let mut type_defs = Vec::new();
        let mut skills = Vec::new();
        let mut pipelines = Vec::new();
        let mut orchestrations = Vec::new();
        let mut mixins = Vec::new();
        let mut packages = Vec::new();

        while !self.at_end() {
            match self.peek_kind() {
                TokenKind::Import => imports.push(self.parse_import()?),
                TokenKind::Type => type_defs.push(self.parse_type_def()?),
                TokenKind::Skill => skills.push(self.parse_skill()?),
                TokenKind::Pipeline => pipelines.push(self.parse_pipeline()?),
                TokenKind::Orchestration => orchestrations.push(self.parse_orchestration()?),
                TokenKind::Mixin => mixins.push(self.parse_mixin()?),
                TokenKind::Package => packages.push(self.parse_package()?),
                TokenKind::Eof => break,
                _ => {
                    let span = self.peek_span();
                    let text = self.peek_text();
                    return Err(SkillSpecError::UnexpectedToken {
                        found: text,
                        expected: "import, type, skill, pipeline, orchestration, mixin, or package"
                            .to_string(),
                        span,
                    });
                }
            }
        }

        Ok(SourceFile {
            imports,
            type_defs,
            skills,
            pipelines,
            orchestrations,
            mixins,
            packages,
        })
    }

    // ── Import ────────────────────────────────────────────────────────

    fn parse_import(&mut self) -> Result<Import> {
        let span = self.peek_span();
        self.expect(TokenKind::Import)?;
        self.expect(TokenKind::LBrace)?;

        let mut symbols = Vec::new();
        loop {
            let name = self.expect_ident()?;
            symbols.push(name);
            if self.peek_kind() == TokenKind::Comma {
                self.advance();
            } else {
                break;
            }
        }

        self.expect(TokenKind::RBrace)?;
        self.expect(TokenKind::From)?;
        let path = self.expect_string_lit()?;

        Ok(Import {
            symbols,
            path,
            span,
        })
    }

    // ── Type definition ───────────────────────────────────────────────

    fn parse_type_def(&mut self) -> Result<TypeDef> {
        let span = self.peek_span();
        self.expect(TokenKind::Type)?;
        let name = self.expect_ident()?;
        self.expect(TokenKind::LBrace)?;
        let fields = self.parse_fields()?;
        self.expect(TokenKind::RBrace)?;

        Ok(TypeDef { name, fields, span })
    }

    // ── Skill ─────────────────────────────────────────────────────────

    fn parse_skill(&mut self) -> Result<Skill> {
        let span = self.peek_span();
        self.expect(TokenKind::Skill)?;
        let name = self.expect_string_lit()?;

        let extends = if self.peek_kind() == TokenKind::Extends {
            self.advance();
            Some(self.expect_string_lit()?)
        } else {
            None
        };

        self.expect(TokenKind::LBrace)?;

        let mut input = None;
        let mut output = None;
        let mut pre = Vec::new();
        let mut post = Vec::new();
        let mut body = Body::default();
        let mut tools = None;
        let mut permissions = None;
        let mut includes = Vec::new();
        let mut tests = Vec::new();

        while self.peek_kind() != TokenKind::RBrace {
            match self.peek_kind() {
                TokenKind::Input => {
                    self.advance();
                    self.expect(TokenKind::LBrace)?;
                    input = Some(self.parse_fields()?);
                    self.expect(TokenKind::RBrace)?;
                }
                TokenKind::Output => {
                    self.advance();
                    self.expect(TokenKind::LBrace)?;
                    output = Some(self.parse_fields()?);
                    self.expect(TokenKind::RBrace)?;
                }
                TokenKind::Pre => {
                    self.advance();
                    self.expect(TokenKind::LBrace)?;
                    pre = self.parse_assertions()?;
                    self.expect(TokenKind::RBrace)?;
                }
                TokenKind::Post => {
                    self.advance();
                    self.expect(TokenKind::LBrace)?;
                    post = self.parse_assertions()?;
                    self.expect(TokenKind::RBrace)?;
                }
                TokenKind::Body => {
                    self.advance();
                    self.expect(TokenKind::LBrace)?;
                    body = self.parse_body()?;
                    self.expect(TokenKind::RBrace)?;
                }
                TokenKind::Context => {
                    // Shorthand: context directly in skill (no body wrapper)
                    body.contexts.push(self.parse_context_block()?);
                }
                TokenKind::Tools => {
                    tools = Some(self.parse_tools_block()?);
                }
                TokenKind::Permissions => {
                    permissions = Some(self.parse_permissions_block()?);
                }
                TokenKind::Include => {
                    self.advance();
                    includes.push(self.expect_ident()?);
                }
                TokenKind::Tests => {
                    tests = self.parse_tests_block()?;
                }
                _ => {
                    let span = self.peek_span();
                    let text = self.peek_text();
                    return Err(SkillSpecError::UnexpectedToken {
                        found: text,
                        expected:
                            "input, output, pre, post, body, context, tools, permissions, include, or tests"
                                .to_string(),
                        span,
                    });
                }
            }
        }

        self.expect(TokenKind::RBrace)?;

        Ok(Skill {
            name,
            extends,
            input,
            output,
            pre,
            post,
            body,
            span,
            tools,
            permissions,
            includes,
            tests,
        })
    }

    // ── Body ──────────────────────────────────────────────────────────

    fn parse_body(&mut self) -> Result<Body> {
        let mut contexts = Vec::new();
        let mut lazy_contexts = Vec::new();
        let mut steps = Vec::new();
        let mut on_error = None;
        let mut directives = PromptDirectives::default();
        let mut source_order = Vec::new();

        while self.peek_kind() != TokenKind::RBrace {
            match self.peek_kind() {
                TokenKind::Lazy => {
                    source_order.push(BodyItemRef::LazyContext(lazy_contexts.len()));
                    lazy_contexts.push(self.parse_lazy_context()?);
                }
                TokenKind::Context => {
                    source_order.push(BodyItemRef::Context(contexts.len()));
                    contexts.push(self.parse_context_block()?);
                }
                TokenKind::Step => {
                    source_order.push(BodyItemRef::Step(steps.len()));
                    steps.push(self.parse_step()?);
                }
                TokenKind::OnError => {
                    self.advance();
                    self.expect(TokenKind::LBrace)?;
                    let mut calls = Vec::new();
                    while self.peek_kind() != TokenKind::RBrace {
                        calls.push(self.parse_use_call()?);
                    }
                    self.expect(TokenKind::RBrace)?;
                    on_error = Some(calls);
                }
                TokenKind::Reasoning => {
                    self.advance();
                    // "reasoning extended" / "reasoning standard" / "reasoning none"
                    let mode = self.expect_ident()?;
                    directives.reasoning = Some(mode);
                }
                TokenKind::Examples => {
                    self.advance();
                    self.expect(TokenKind::LBrace)?;
                    while self.peek_kind() == TokenKind::Example {
                        directives.examples.push(self.parse_prompt_example()?);
                    }
                    self.expect(TokenKind::RBrace)?;
                }
                TokenKind::Format => {
                    directives.format = Some(self.parse_format_directive()?);
                }
                TokenKind::Reinforce => {
                    directives.reinforcements.push(self.parse_reinforcement()?);
                }
                TokenKind::Sampling => {
                    directives.sampling = Some(self.parse_sampling_directive()?);
                }
                TokenKind::Persona => {
                    self.advance();
                    self.expect(TokenKind::LBrace)?;
                    let text = self.parse_prose_content()?;
                    self.expect(TokenKind::RBrace)?;
                    directives.persona = Some(text);
                }
                _ => {
                    let span = self.peek_span();
                    let text = self.peek_text();
                    return Err(SkillSpecError::UnexpectedToken {
                        found: text,
                        expected: "context, lazy context, step, on_error, or prompt directive"
                            .to_string(),
                        span,
                    });
                }
            }
        }

        Ok(Body {
            contexts,
            lazy_contexts,
            steps,
            on_error,
            directives,
            source_order,
        })
    }

    // ── Context block ─────────────────────────────────────────────────

    fn parse_context_block(&mut self) -> Result<ContextBlock> {
        let span = self.peek_span();
        self.expect(TokenKind::Context)?;

        let mut priority = None;
        let mut when = None;
        let mut decay = None;

        // Optional parameters in parens: context(priority: N, when: expr, decay: N)
        if self.peek_kind() == TokenKind::LParen {
            self.advance();
            while self.peek_kind() != TokenKind::RParen {
                let param_name = self.expect_ident()?;
                self.expect(TokenKind::Colon)?;
                match param_name.as_str() {
                    "priority" => {
                        priority = Some(self.expect_int_lit()? as u8);
                    }
                    "when" => {
                        when = Some(self.parse_expr()?);
                    }
                    "decay" => {
                        decay = Some(self.expect_float_lit()?);
                    }
                    _ => {
                        return Err(SkillSpecError::UnexpectedToken {
                            found: param_name,
                            expected: "priority, when, or decay".to_string(),
                            span,
                        });
                    }
                }
                if self.peek_kind() == TokenKind::Comma {
                    self.advance();
                }
            }
            self.expect(TokenKind::RParen)?;
        }

        self.expect(TokenKind::LBrace)?;
        let text = self.parse_prose_content()?;
        self.expect(TokenKind::RBrace)?;

        Ok(ContextBlock {
            priority,
            when,
            decay,
            text,
            span,
        })
    }

    // ── Step ──────────────────────────────────────────────────────────

    fn parse_step(&mut self) -> Result<Step> {
        let span = self.peek_span();
        self.expect(TokenKind::Step)?;
        let name = self.expect_ident()?;
        self.expect(TokenKind::LBrace)?;

        let mut requires = None;
        let mut when = None;
        let mut use_call = None;
        let mut lets = Vec::new();
        let mut emit = false;
        let mut contexts = Vec::new();
        let mut loads = Vec::new();

        while self.peek_kind() != TokenKind::RBrace {
            match self.peek_kind() {
                TokenKind::Requires => {
                    self.advance();
                    requires = Some(self.parse_dependency()?);
                }
                TokenKind::When => {
                    self.advance();
                    when = Some(self.parse_expr()?);
                }
                TokenKind::Use => {
                    use_call = Some(self.parse_use_call()?);
                }
                TokenKind::Let => {
                    lets.push(self.parse_let()?);
                }
                TokenKind::Emit => {
                    self.advance();
                    emit = true;
                    // "emit output" — consume the "output" keyword/ident
                    self.expect_specific_ident("output")?;
                }
                TokenKind::Context => {
                    contexts.push(self.parse_context_block()?);
                }
                TokenKind::Load => {
                    self.advance();
                    let name = self.expect_string_lit()?;
                    loads.push(name);
                }
                _ => {
                    let s = self.peek_span();
                    let text = self.peek_text();
                    return Err(SkillSpecError::UnexpectedToken {
                        found: text,
                        expected: "requires, when, use, let, emit, load, or context".to_string(),
                        span: s,
                    });
                }
            }
        }

        self.expect(TokenKind::RBrace)?;

        Ok(Step {
            name,
            requires,
            when,
            use_call,
            lets,
            emit,
            contexts,
            span,
            loads,
        })
    }

    // ── Dependency ────────────────────────────────────────────────────

    fn parse_dependency(&mut self) -> Result<Dependency> {
        if self.peek_kind() == TokenKind::AllSteps {
            self.advance();
            return Ok(Dependency::AllSteps);
        }

        let first = self.expect_ident()?;

        if self.peek_kind() == TokenKind::Amp {
            // All: a & b & c
            let mut names = vec![first];
            while self.peek_kind() == TokenKind::Amp {
                self.advance();
                names.push(self.expect_ident()?);
            }
            Ok(Dependency::All(names))
        } else if self.peek_kind() == TokenKind::Pipe {
            // Any: a | b | c
            let mut names = vec![first];
            while self.peek_kind() == TokenKind::Pipe {
                self.advance();
                names.push(self.expect_ident()?);
            }
            Ok(Dependency::Any(names))
        } else {
            Ok(Dependency::Single(first))
        }
    }

    // ── Use call ──────────────────────────────────────────────────────

    fn parse_use_call(&mut self) -> Result<UseCall> {
        let span = self.peek_span();
        self.expect(TokenKind::Use)?;
        let skill_name = self.expect_ident()?;
        self.expect(TokenKind::LParen)?;
        let args = self.parse_named_args()?;
        self.expect(TokenKind::RParen)?;

        Ok(UseCall {
            skill_name,
            args,
            span,
        })
    }

    // ── Let binding ───────────────────────────────────────────────────

    fn parse_let(&mut self) -> Result<LetBinding> {
        let span = self.peek_span();
        self.expect(TokenKind::Let)?;
        let name = self.expect_ident()?;
        self.expect(TokenKind::Eq)?;
        let value = self.parse_expr()?;

        Ok(LetBinding { name, value, span })
    }

    // ── Assertions ────────────────────────────────────────────────────

    fn parse_assertions(&mut self) -> Result<Vec<Assertion>> {
        let mut assertions = Vec::new();
        while self.peek_kind() == TokenKind::Assert {
            let span = self.peek_span();
            self.advance(); // consume assert

            // Optional `when` guard: assert when <guard> <condition> message "text"
            let mut when_guard = None;
            if self.peek_kind() == TokenKind::When {
                self.advance();
                when_guard = Some(self.parse_expr()?);
            }

            let condition = self.parse_expr()?;
            self.expect(TokenKind::Message)?;
            let message = self.expect_string_lit()?;

            assertions.push(Assertion {
                condition,
                when_guard,
                message,
                span,
            });
        }
        Ok(assertions)
    }

    // ── Fields ────────────────────────────────────────────────────────

    fn parse_fields(&mut self) -> Result<Vec<Field>> {
        let mut fields = Vec::new();
        while self.peek_kind() != TokenKind::RBrace {
            let span = self.peek_span();
            let name = self.expect_ident()?;

            let optional = if self.peek_kind() == TokenKind::Question {
                self.advance();
                true
            } else {
                false
            };

            self.expect(TokenKind::Colon)?;
            let ty = self.parse_type_expr()?;

            let default = if self.peek_kind() == TokenKind::Eq {
                self.advance();
                Some(self.parse_primary_expr()?)
            } else {
                None
            };

            fields.push(Field {
                name,
                ty,
                optional,
                default,
                span,
            });
        }
        Ok(fields)
    }

    // ── Type expression ───────────────────────────────────────────────

    fn parse_type_expr(&mut self) -> Result<TypeExpr> {
        let base = match self.peek_kind() {
            TokenKind::StringType => {
                self.advance();
                TypeExpr::String
            }
            TokenKind::IntType => {
                self.advance();
                TypeExpr::Int
            }
            TokenKind::FloatType => {
                self.advance();
                TypeExpr::Float
            }
            TokenKind::BoolType => {
                self.advance();
                TypeExpr::Bool
            }
            TokenKind::Enum => {
                self.advance();
                self.expect(TokenKind::LParen)?;
                let mut variants = Vec::new();
                loop {
                    variants.push(self.expect_string_lit()?);
                    if self.peek_kind() == TokenKind::Comma {
                        self.advance();
                    } else {
                        break;
                    }
                }
                self.expect(TokenKind::RParen)?;
                TypeExpr::Enum(variants)
            }
            TokenKind::Map => {
                self.advance();
                self.expect(TokenKind::Lt)?;
                let key = self.parse_type_expr()?;
                self.expect(TokenKind::Comma)?;
                let value = self.parse_type_expr()?;
                self.expect(TokenKind::Gt)?;
                TypeExpr::Map(Box::new(key), Box::new(value))
            }
            TokenKind::Ident(ref name) => {
                let n = name.clone();
                self.advance();
                TypeExpr::Named(n)
            }
            _ => {
                let span = self.peek_span();
                let text = self.peek_text();
                return Err(SkillSpecError::UnexpectedToken {
                    found: text,
                    expected: "type (string, int, float, bool, enum, map, or named type)"
                        .to_string(),
                    span,
                });
            }
        };

        // Array suffix: []
        if self.peek_kind() == TokenKind::LBracket {
            self.advance();
            self.expect(TokenKind::RBracket)?;
            Ok(TypeExpr::Array(Box::new(base)))
        } else {
            Ok(base)
        }
    }

    // ── Expressions ───────────────────────────────────────────────────

    fn parse_expr(&mut self) -> Result<Expr> {
        // Lowest precedence: || (logical OR)
        let mut left = self.parse_and_expr()?;

        while self.peek_kind() == TokenKind::PipePipe {
            self.advance();
            let right = self.parse_and_expr()?;
            left = Expr::BinOp(Box::new(left), BinOp::Or, Box::new(right));
        }

        Ok(left)
    }

    fn parse_and_expr(&mut self) -> Result<Expr> {
        // Middle precedence: && (logical AND)
        let mut left = self.parse_comparison_expr()?;

        while self.peek_kind() == TokenKind::AmpAmp {
            self.advance();
            let right = self.parse_comparison_expr()?;
            left = Expr::BinOp(Box::new(left), BinOp::And, Box::new(right));
        }

        Ok(left)
    }

    fn parse_comparison_expr(&mut self) -> Result<Expr> {
        // Highest binary precedence: comparison operators
        let left = self.parse_primary_expr()?;

        let op = match self.peek_kind() {
            TokenKind::EqEq => Some(BinOp::Eq),
            TokenKind::NotEq => Some(BinOp::NotEq),
            TokenKind::Lt => Some(BinOp::Lt),
            TokenKind::Gt => Some(BinOp::Gt),
            TokenKind::LtEq => Some(BinOp::LtEq),
            TokenKind::GtEq => Some(BinOp::GtEq),
            _ => None,
        };

        if let Some(op) = op {
            self.advance();
            let right = self.parse_primary_expr()?;
            Ok(Expr::BinOp(Box::new(left), op, Box::new(right)))
        } else {
            Ok(left)
        }
    }

    fn parse_primary_expr(&mut self) -> Result<Expr> {
        let expr = match self.peek_kind() {
            TokenKind::StringLit(ref s) => {
                let val = s.clone();
                self.advance();
                Expr::StringLit(val)
            }
            TokenKind::TripleString(ref s) => {
                let val = s.clone();
                self.advance();
                Expr::StringLit(val)
            }
            TokenKind::IntLit(n) => {
                self.advance();
                Expr::IntLit(n)
            }
            TokenKind::FloatLit(f) => {
                self.advance();
                Expr::FloatLit(f)
            }
            TokenKind::BoolLit(b) => {
                self.advance();
                Expr::BoolLit(b)
            }
            TokenKind::Ident(ref name) => {
                let n = name.clone();
                self.advance();
                // Check for field access chain: a.b.c
                let mut expr = Expr::Ident(n);
                while self.peek_kind() == TokenKind::Dot {
                    self.advance();
                    let field = self.expect_ident()?;
                    expr = Expr::FieldAccess(Box::new(expr), field);
                }
                expr
            }
            TokenKind::LBracket => {
                self.advance();
                let mut elements = Vec::new();
                while self.peek_kind() != TokenKind::RBracket {
                    elements.push(self.parse_expr()?);
                    if self.peek_kind() == TokenKind::Comma {
                        self.advance();
                    }
                }
                self.expect(TokenKind::RBracket)?;
                Expr::ArrayLit(elements)
            }
            TokenKind::Dot => {
                // Leading dot: .field — shorthand for _item.field (used in where-clauses)
                self.advance();
                let field = self.expect_ident()?;
                let mut expr = Expr::FieldAccess(Box::new(Expr::Ident("_item".into())), field);
                while self.peek_kind() == TokenKind::Dot {
                    self.advance();
                    let next_field = self.expect_ident()?;
                    expr = Expr::FieldAccess(Box::new(expr), next_field);
                }
                expr
            }
            TokenKind::Bang => {
                self.advance();
                let inner = self.parse_primary_expr()?;
                Expr::Not(Box::new(inner))
            }
            TokenKind::Interpolation(ref s) => {
                let val = s.clone();
                self.advance();
                Expr::Interpolated(val)
            }
            // Handle keyword tokens that might appear as identifiers in expression context
            // e.g., "input.branch" — input is a keyword but used as an identifier here
            TokenKind::Input => {
                self.advance();
                let mut expr = Expr::Ident("input".to_string());
                while self.peek_kind() == TokenKind::Dot {
                    self.advance();
                    let field = self.expect_ident()?;
                    expr = Expr::FieldAccess(Box::new(expr), field);
                }
                expr
            }
            TokenKind::Output => {
                self.advance();
                let mut expr = Expr::Ident("output".to_string());
                while self.peek_kind() == TokenKind::Dot {
                    self.advance();
                    let field = self.expect_ident()?;
                    expr = Expr::FieldAccess(Box::new(expr), field);
                }
                expr
            }
            _ => {
                let span = self.peek_span();
                let text = self.peek_text();
                return Err(SkillSpecError::UnexpectedToken {
                    found: text,
                    expected: "expression".to_string(),
                    span,
                });
            }
        };

        Ok(expr)
    }

    // ── Named arguments ───────────────────────────────────────────────

    fn parse_named_args(&mut self) -> Result<Vec<(String, Expr)>> {
        let mut args = Vec::new();
        while self.peek_kind() != TokenKind::RParen {
            let name = self.expect_ident()?;
            self.expect(TokenKind::Colon)?;
            let value = self.parse_expr()?;
            args.push((name, value));
            if self.peek_kind() == TokenKind::Comma {
                self.advance();
            }
        }
        Ok(args)
    }

    // ── Prose content ─────────────────────────────────────────────────

    fn parse_prose_content(&mut self) -> Result<String> {
        match self.peek_kind() {
            TokenKind::StringLit(ref s) => {
                let val = s.clone();
                self.advance();
                Ok(val)
            }
            TokenKind::TripleString(ref s) => {
                let val = s.clone();
                self.advance();
                Ok(val)
            }
            _ => {
                let span = self.peek_span();
                let text = self.peek_text();
                Err(SkillSpecError::UnexpectedToken {
                    found: text,
                    expected: "string literal or triple-quoted string".to_string(),
                    span,
                })
            }
        }
    }

    // ── Lazy context ──────────────────────────────────────────────────

    fn parse_lazy_context(&mut self) -> Result<LazyContext> {
        let span = self.peek_span();
        self.expect(TokenKind::Lazy)?;
        self.expect(TokenKind::Context)?;
        let name = self.expect_string_lit()?;

        // Optional priority in parens: (priority: N)
        let mut priority = None;
        if self.peek_kind() == TokenKind::LParen {
            self.advance();
            while self.peek_kind() != TokenKind::RParen {
                let param_name = self.expect_ident()?;
                self.expect(TokenKind::Colon)?;
                match param_name.as_str() {
                    "priority" => {
                        priority = Some(self.expect_int_lit()? as u8);
                    }
                    _ => {
                        return Err(SkillSpecError::UnexpectedToken {
                            found: param_name,
                            expected: "priority".to_string(),
                            span,
                        });
                    }
                }
                if self.peek_kind() == TokenKind::Comma {
                    self.advance();
                }
            }
            self.expect(TokenKind::RParen)?;
        }

        self.expect(TokenKind::LBrace)?;

        // Parse summary (required)
        self.expect(TokenKind::Summary)?;
        let summary = self.expect_string_lit()?;

        // Parse content: ref, index, or inline triple-string / string
        let content = if self.peek_kind() == TokenKind::Ref {
            self.advance();
            let path = self.expect_string_lit()?;
            LazyContent::Ref(path)
        } else if self.peek_kind() == TokenKind::Index {
            self.advance();
            self.expect(TokenKind::LBrace)?;
            let mut sections = Vec::new();
            while self.peek_kind() == TokenKind::Section {
                self.advance();
                let sec_name = self.expect_string_lit()?;
                self.expect(TokenKind::LBrace)?;

                self.expect(TokenKind::Summary)?;
                let sec_summary = self.expect_string_lit()?;

                self.expect(TokenKind::Ref)?;
                let ref_path = self.expect_string_lit()?;

                self.expect(TokenKind::RBrace)?;

                sections.push(IndexSection {
                    name: sec_name,
                    summary: sec_summary,
                    ref_path,
                });
            }
            self.expect(TokenKind::RBrace)?;
            LazyContent::Index(sections)
        } else {
            let text = self.parse_prose_content()?;
            LazyContent::Inline(text)
        };

        self.expect(TokenKind::RBrace)?;

        Ok(LazyContext {
            name,
            priority,
            summary,
            content,
            span,
        })
    }

    // ── Tools block ──────────────────────────────────────────────────

    fn parse_tools_block(&mut self) -> Result<ToolsBlock> {
        self.expect(TokenKind::Tools)?;
        self.expect(TokenKind::LBrace)?;

        let mut required = Vec::new();
        let mut optional = Vec::new();

        while self.peek_kind() != TokenKind::RBrace {
            let is_optional = match self.peek_kind() {
                TokenKind::Require => {
                    self.advance();
                    false
                }
                TokenKind::Optional => {
                    self.advance();
                    true
                }
                _ => {
                    let span = self.peek_span();
                    let text = self.peek_text();
                    return Err(SkillSpecError::UnexpectedToken {
                        found: text,
                        expected: "require or optional".to_string(),
                        span,
                    });
                }
            };

            let decl = self.parse_tool_decl()?;

            if is_optional {
                optional.push(decl);
            } else {
                required.push(decl);
            }
        }

        self.expect(TokenKind::RBrace)?;

        Ok(ToolsBlock { required, optional })
    }

    fn parse_tool_decl(&mut self) -> Result<ToolDecl> {
        if self.peek_kind() == TokenKind::Mcp {
            // MCP tool: mcp("name") { methods }
            self.advance();
            self.expect(TokenKind::LParen)?;
            let mcp_name = self.expect_string_lit()?;
            self.expect(TokenKind::RParen)?;

            let mut methods = Vec::new();
            if self.peek_kind() == TokenKind::LBrace {
                self.advance();
                while self.peek_kind() != TokenKind::RBrace {
                    methods.push(self.parse_tool_method()?);
                }
                self.expect(TokenKind::RBrace)?;
            }

            Ok(ToolDecl {
                kind: ToolKind::Mcp(mcp_name.clone()),
                name: mcp_name,
                methods,
                allow: Vec::new(),
                deny: Vec::new(),
            })
        } else {
            // Builtin tool: just a name (Bash, Read, Edit, etc.)
            let name = self.expect_ident()?;
            Ok(ToolDecl {
                kind: ToolKind::Builtin,
                name,
                methods: Vec::new(),
                allow: Vec::new(),
                deny: Vec::new(),
            })
        }
    }

    fn parse_tool_method(&mut self) -> Result<ToolMethod> {
        let name = self.expect_ident()?;
        self.expect(TokenKind::LParen)?;

        let mut params = Vec::new();
        while self.peek_kind() != TokenKind::RParen {
            let pname = self.expect_ident()?;
            self.expect(TokenKind::Colon)?;
            let ty = self.parse_type_expr()?;
            // For now, all method params are required
            params.push((pname, ty, false));
            if self.peek_kind() == TokenKind::Comma {
                self.advance();
            }
        }
        self.expect(TokenKind::RParen)?;

        self.expect(TokenKind::Arrow)?;

        // Return type — handle "void" as a special named type
        let return_type = self.parse_type_expr()?;

        Ok(ToolMethod {
            name,
            params,
            return_type,
        })
    }

    // ── Permissions block ────────────────────────────────────────────

    fn parse_permissions_block(&mut self) -> Result<PermissionsBlock> {
        self.expect(TokenKind::Permissions)?;
        self.expect(TokenKind::LBrace)?;

        let mut filesystem = None;
        let mut network = None;
        let mut secrets = Vec::new();

        while self.peek_kind() != TokenKind::RBrace {
            let key = self.expect_ident()?;
            self.expect(TokenKind::Colon)?;

            match key.as_str() {
                "filesystem" => {
                    // filesystem: read_write("pattern", ...)
                    let mode = self.expect_ident()?;
                    self.expect(TokenKind::LParen)?;
                    let mut patterns = Vec::new();
                    while self.peek_kind() != TokenKind::RParen {
                        patterns.push(self.expect_string_lit()?);
                        if self.peek_kind() == TokenKind::Comma {
                            self.advance();
                        }
                    }
                    self.expect(TokenKind::RParen)?;
                    filesystem = Some((mode, patterns));
                }
                "network" => {
                    // network: outbound("host", ...)
                    let mode = self.expect_ident()?;
                    self.expect(TokenKind::LParen)?;
                    let mut hosts = Vec::new();
                    while self.peek_kind() != TokenKind::RParen {
                        hosts.push(self.expect_string_lit()?);
                        if self.peek_kind() == TokenKind::Comma {
                            self.advance();
                        }
                    }
                    self.expect(TokenKind::RParen)?;
                    network = Some((mode, hosts));
                }
                "secrets" => {
                    // secrets: ["TOKEN1", "TOKEN2"]
                    self.expect(TokenKind::LBracket)?;
                    while self.peek_kind() != TokenKind::RBracket {
                        secrets.push(self.expect_string_lit()?);
                        if self.peek_kind() == TokenKind::Comma {
                            self.advance();
                        }
                    }
                    self.expect(TokenKind::RBracket)?;
                }
                _ => {
                    let span = self.peek_span();
                    return Err(SkillSpecError::UnexpectedToken {
                        found: key,
                        expected: "filesystem, network, or secrets".to_string(),
                        span,
                    });
                }
            }
        }

        self.expect(TokenKind::RBrace)?;

        Ok(PermissionsBlock {
            filesystem,
            network,
            secrets,
        })
    }

    // ── Prompt directive helpers ─────────────────────────────────────

    fn parse_prompt_example(&mut self) -> Result<PromptExample> {
        self.expect(TokenKind::Example)?;
        let name = self.expect_string_lit()?;
        self.expect(TokenKind::LBrace)?;

        let mut input = String::new();
        let mut output = String::new();
        let mut note = None;

        while self.peek_kind() != TokenKind::RBrace {
            let key = self.expect_ident()?;
            self.expect(TokenKind::Colon)?;
            match key.as_str() {
                "input" => {
                    input = self.parse_prose_content()?;
                }
                "output" => {
                    output = self.parse_prose_or_object_content()?;
                }
                "note" => {
                    note = Some(self.parse_prose_content()?);
                }
                _ => {
                    let span = self.peek_span();
                    return Err(SkillSpecError::UnexpectedToken {
                        found: key,
                        expected: "input, output, or note".to_string(),
                        span,
                    });
                }
            }
        }
        self.expect(TokenKind::RBrace)?;

        Ok(PromptExample {
            name,
            input,
            output,
            note,
        })
    }

    /// Parse content that can be either a string literal or a braced object literal
    /// (serialised as its raw text for now).
    fn parse_prose_or_object_content(&mut self) -> Result<String> {
        if self.peek_kind() == TokenKind::LBrace {
            // Consume everything inside { ... } as a flat string representation
            self.advance(); // consume {
            let mut parts: Vec<String> = Vec::new();
            while self.peek_kind() != TokenKind::RBrace {
                let key = self.expect_ident()?;
                self.expect(TokenKind::Colon)?;
                let val = self.parse_primary_expr()?;
                parts.push(format!("{}: {:?}", key, val));
                if self.peek_kind() == TokenKind::Comma {
                    self.advance();
                }
            }
            self.expect(TokenKind::RBrace)?;
            Ok(format!("{{ {} }}", parts.join(", ")))
        } else {
            self.parse_prose_content()
        }
    }

    fn parse_format_directive(&mut self) -> Result<FormatDirective> {
        self.expect(TokenKind::Format)?;
        self.expect(TokenKind::LBrace)?;

        let mut style = String::new();
        let mut structure = String::new();

        while self.peek_kind() != TokenKind::RBrace {
            let key = self.expect_ident()?;
            self.expect(TokenKind::Colon)?;
            match key.as_str() {
                "style" => {
                    style = self.expect_ident()?;
                }
                "structure" => {
                    structure = self.expect_ident()?;
                }
                _ => {
                    let span = self.peek_span();
                    return Err(SkillSpecError::UnexpectedToken {
                        found: key,
                        expected: "style or structure".to_string(),
                        span,
                    });
                }
            }
        }
        self.expect(TokenKind::RBrace)?;

        Ok(FormatDirective { style, structure })
    }

    fn parse_reinforcement(&mut self) -> Result<Reinforcement> {
        self.expect(TokenKind::Reinforce)?;

        let trigger = if self.peek_kind() == TokenKind::Every {
            self.advance();
            let n = self.expect_int_lit()?;
            // consume "steps" ident
            self.expect_specific_ident("steps")?;
            ReinforceTrigger::EveryNSteps(n)
        } else if self.peek_kind() == TokenKind::On {
            self.advance();
            // "on context_shift" etc.
            let event = self.expect_ident()?;
            if event == "context_shift" {
                ReinforceTrigger::OnContextShift
            } else {
                // Treat as a when condition for extensibility
                ReinforceTrigger::WhenCondition(Expr::Ident(event))
            }
        } else if self.peek_kind() == TokenKind::When {
            self.advance();
            let cond = self.parse_expr()?;
            ReinforceTrigger::WhenCondition(cond)
        } else {
            let span = self.peek_span();
            let text = self.peek_text();
            return Err(SkillSpecError::UnexpectedToken {
                found: text,
                expected: "every, on, or when".to_string(),
                span,
            });
        };

        self.expect(TokenKind::LBrace)?;
        let text = self.parse_prose_content()?;
        self.expect(TokenKind::RBrace)?;

        Ok(Reinforcement { trigger, text })
    }

    fn parse_sampling_directive(&mut self) -> Result<SamplingDirective> {
        self.expect(TokenKind::Sampling)?;
        self.expect(TokenKind::LBrace)?;

        let mut temperature = None;
        let mut top_p = None;

        while self.peek_kind() != TokenKind::RBrace {
            let key = self.expect_ident()?;
            self.expect(TokenKind::Colon)?;
            match key.as_str() {
                "temperature" => {
                    temperature = Some(self.expect_number_as_f64()?);
                }
                "top_p" => {
                    top_p = Some(self.expect_number_as_f64()?);
                }
                _ => {
                    let span = self.peek_span();
                    return Err(SkillSpecError::UnexpectedToken {
                        found: key,
                        expected: "temperature or top_p".to_string(),
                        span,
                    });
                }
            }
        }
        self.expect(TokenKind::RBrace)?;

        Ok(SamplingDirective { temperature, top_p })
    }

    // ── Tests block ──────────────────────────────────────────────────

    fn parse_tests_block(&mut self) -> Result<Vec<TestBlock>> {
        self.expect(TokenKind::Tests)?;
        self.expect(TokenKind::LBrace)?;

        let mut tests = Vec::new();
        while self.peek_kind() == TokenKind::Test {
            tests.push(self.parse_single_test()?);
        }

        self.expect(TokenKind::RBrace)?;
        Ok(tests)
    }

    fn parse_single_test(&mut self) -> Result<TestBlock> {
        let span = self.peek_span();
        self.expect(TokenKind::Test)?;
        let name = self.expect_string_lit()?;
        self.expect(TokenKind::LBrace)?;

        let mut given = Vec::new();
        let mut mocks = Vec::new();
        let mut expectations = Vec::new();
        let mut confidence = None;
        let mut runs = None;
        let mut snapshot = None;

        while self.peek_kind() != TokenKind::RBrace {
            match self.peek_kind() {
                TokenKind::Given => {
                    self.advance();
                    self.expect(TokenKind::LBrace)?;
                    while self.peek_kind() != TokenKind::RBrace {
                        let key = self.expect_ident()?;
                        self.expect(TokenKind::Colon)?;
                        let value = self.parse_expr()?;
                        given.push((key, value));
                    }
                    self.expect(TokenKind::RBrace)?;
                }
                TokenKind::Mock => {
                    mocks.push(self.parse_mock_decl()?);
                }
                TokenKind::Expect => {
                    self.advance();
                    self.expect(TokenKind::LBrace)?;
                    while self.peek_kind() != TokenKind::RBrace {
                        expectations.push(self.parse_expectation()?);
                    }
                    self.expect(TokenKind::RBrace)?;
                }
                TokenKind::Confidence => {
                    self.advance();
                    confidence = Some(self.expect_number_as_f64()?);
                }
                TokenKind::Runs => {
                    self.advance();
                    runs = Some(self.expect_int_lit()?);
                }
                TokenKind::Snapshot => {
                    self.advance();
                    snapshot = Some(self.expect_string_lit()?);
                }
                _ => {
                    let s = self.peek_span();
                    let text = self.peek_text();
                    return Err(SkillSpecError::UnexpectedToken {
                        found: text,
                        expected: "given, mock, expect, confidence, runs, or snapshot".to_string(),
                        span: s,
                    });
                }
            }
        }

        self.expect(TokenKind::RBrace)?;

        Ok(TestBlock {
            name,
            given,
            mocks,
            expectations,
            confidence,
            runs,
            snapshot,
            span,
        })
    }

    fn parse_mock_decl(&mut self) -> Result<MockDecl> {
        self.expect(TokenKind::Mock)?;

        // Parse dotted tool path: tools.mcp.github
        let mut path = self.expect_ident()?;
        while self.peek_kind() == TokenKind::Dot {
            self.advance();
            let segment = self.expect_ident()?;
            path.push('.');
            path.push_str(&segment);
        }

        // Check for shorthand: `mock tools.path: unavailable|failing|slow`
        if self.peek_kind() == TokenKind::Colon {
            self.advance();
            let mock_type = match self.peek_kind() {
                TokenKind::Unavailable => {
                    self.advance();
                    MockType::Unavailable
                }
                TokenKind::Failing => {
                    self.advance();
                    // Optional reason string
                    let reason = if let TokenKind::StringLit(_) = self.peek_kind() {
                        self.expect_string_lit()?
                    } else {
                        String::new()
                    };
                    MockType::Failing(reason)
                }
                TokenKind::Slow => {
                    self.advance();
                    let duration = if let TokenKind::StringLit(_) = self.peek_kind() {
                        self.expect_string_lit()?
                    } else {
                        String::new()
                    };
                    MockType::Slow(duration)
                }
                _ => {
                    let span = self.peek_span();
                    let text = self.peek_text();
                    return Err(SkillSpecError::UnexpectedToken {
                        found: text,
                        expected: "unavailable, failing, or slow".to_string(),
                        span,
                    });
                }
            };
            return Ok(MockDecl {
                tool_path: path,
                mock_type,
            });
        }

        // Braced form: mock tools.path { method(...) -> response }
        self.expect(TokenKind::LBrace)?;
        let mut responses = Vec::new();

        while self.peek_kind() != TokenKind::RBrace {
            let method = self.expect_ident()?;
            self.expect(TokenKind::LParen)?;
            let args = self.parse_named_args()?;
            self.expect(TokenKind::RParen)?;
            self.expect(TokenKind::Arrow)?;
            let response = self.parse_expr()?;
            responses.push(MockResponse {
                method,
                args,
                response,
            });
        }

        self.expect(TokenKind::RBrace)?;

        Ok(MockDecl {
            tool_path: path,
            mock_type: MockType::Responses(responses),
        })
    }

    fn parse_expectation(&mut self) -> Result<Expectation> {
        // Parse dotted path: output.findings.length
        let mut path = self.expect_ident()?;
        while self.peek_kind() == TokenKind::Dot {
            self.advance();
            let segment = self.expect_ident()?;
            path.push('.');
            path.push_str(&segment);
        }

        self.expect(TokenKind::Colon)?;

        // Parse assertion
        let assertion = match self.peek_kind() {
            TokenKind::Equals => {
                self.advance();
                self.expect(TokenKind::LParen)?;
                let value = self.parse_expr()?;
                self.expect(TokenKind::RParen)?;
                AssertionExpr::Equals(value)
            }
            TokenKind::Contains => {
                self.advance();
                self.expect(TokenKind::LParen)?;
                // Check for where-clause: contains(where: .field == "value")
                if self.is_where_keyword() {
                    self.advance(); // consume "where"
                    self.expect(TokenKind::Colon)?;
                    let expr = self.parse_expr()?;
                    self.expect(TokenKind::RParen)?;
                    AssertionExpr::ContainsWhere(expr)
                } else {
                    let value = self.parse_expr()?;
                    self.expect(TokenKind::RParen)?;
                    AssertionExpr::Contains(value)
                }
            }
            TokenKind::Matches => {
                self.advance();
                self.expect(TokenKind::LParen)?;
                let pattern = self.expect_string_lit()?;
                self.expect(TokenKind::RParen)?;
                AssertionExpr::Matches(pattern)
            }
            TokenKind::Resembles => {
                self.advance();
                self.expect(TokenKind::LParen)?;
                let desc = self.expect_string_lit()?;
                self.expect(TokenKind::RParen)?;
                AssertionExpr::Resembles(desc)
            }
            TokenKind::Satisfies => {
                self.advance();
                self.expect(TokenKind::LParen)?;
                let desc = self.expect_string_lit()?;
                self.expect(TokenKind::RParen)?;
                AssertionExpr::Satisfies(desc)
            }
            TokenKind::Between => {
                self.advance();
                self.expect(TokenKind::LParen)?;
                let low = self.parse_expr()?;
                self.expect(TokenKind::Comma)?;
                let high = self.parse_expr()?;
                self.expect(TokenKind::RParen)?;
                AssertionExpr::Between(low, high)
            }
            // Comparison operators: >= N, <= N, > N, < N, == val
            TokenKind::GtEq => {
                self.advance();
                let val = self.parse_expr()?;
                AssertionExpr::Comparison(BinOp::GtEq, val)
            }
            TokenKind::LtEq => {
                self.advance();
                let val = self.parse_expr()?;
                AssertionExpr::Comparison(BinOp::LtEq, val)
            }
            TokenKind::Gt => {
                self.advance();
                let val = self.parse_expr()?;
                AssertionExpr::Comparison(BinOp::Gt, val)
            }
            TokenKind::Lt => {
                self.advance();
                let val = self.parse_expr()?;
                AssertionExpr::Comparison(BinOp::Lt, val)
            }
            TokenKind::EqEq => {
                self.advance();
                let val = self.parse_expr()?;
                AssertionExpr::Comparison(BinOp::Eq, val)
            }
            // Quantifier assertions: all(where: ...) and none(where: ...)
            TokenKind::Ident(ref s) if s == "all" || s == "none" => {
                let kind = if s == "all" { "all" } else { "none" };
                let kind = kind.to_string();
                self.advance();
                self.expect(TokenKind::LParen)?;
                // Expect where: keyword
                if !self.is_where_keyword() {
                    let span = self.peek_span();
                    let text = self.peek_text();
                    return Err(SkillSpecError::UnexpectedToken {
                        found: text,
                        expected: "where".to_string(),
                        span,
                    });
                }
                self.advance(); // consume "where"
                self.expect(TokenKind::Colon)?;
                let expr = self.parse_expr()?;
                self.expect(TokenKind::RParen)?;
                if kind == "all" {
                    AssertionExpr::AllWhere(expr)
                } else {
                    AssertionExpr::NoneWhere(expr)
                }
            }
            _ => {
                let span = self.peek_span();
                let text = self.peek_text();
                return Err(SkillSpecError::UnexpectedToken {
                    found: text,
                    expected: "equals, contains, matches, resembles, satisfies, between, all, none, or comparison operator".to_string(),
                    span,
                });
            }
        };

        Ok(Expectation { path, assertion })
    }

    // ── Package ──────────────────────────────────────────────────────

    fn parse_package(&mut self) -> Result<Package> {
        let span = self.peek_span();
        self.expect(TokenKind::Package)?;
        let name = self.expect_string_lit()?;
        self.expect(TokenKind::LBrace)?;

        let mut version = String::new();
        let mut description = String::new();
        let mut exports = Vec::new();

        while self.peek_kind() != TokenKind::RBrace {
            match self.peek_kind() {
                TokenKind::Version => {
                    self.advance();
                    self.expect(TokenKind::Eq)?;
                    version = self.expect_string_lit()?;
                }
                TokenKind::Description => {
                    self.advance();
                    self.expect(TokenKind::Eq)?;
                    description = self.expect_string_lit()?;
                }
                TokenKind::Exports => {
                    self.advance();
                    self.expect(TokenKind::LBrace)?;
                    while self.peek_kind() != TokenKind::RBrace {
                        exports.push(self.expect_ident()?);
                    }
                    self.expect(TokenKind::RBrace)?;
                }
                _ => {
                    let s = self.peek_span();
                    let text = self.peek_text();
                    return Err(SkillSpecError::UnexpectedToken {
                        found: text,
                        expected: "version, description, or exports".to_string(),
                        span: s,
                    });
                }
            }
        }

        self.expect(TokenKind::RBrace)?;

        Ok(Package {
            name,
            version,
            description,
            exports,
            span,
        })
    }

    // ── Pipeline ─────────────────────────────────────────────────────

    fn parse_pipeline(&mut self) -> Result<Pipeline> {
        let span = self.peek_span();
        self.expect(TokenKind::Pipeline)?;
        let name = self.expect_string_lit()?;
        self.expect(TokenKind::LBrace)?;

        let mut input = None;
        let mut output = None;
        let mut stages = Vec::new();
        let mut on_error = None;
        let mut timeout = None;

        while self.peek_kind() != TokenKind::RBrace {
            match self.peek_kind() {
                TokenKind::Input => {
                    self.advance();
                    self.expect(TokenKind::LBrace)?;
                    input = Some(self.parse_fields()?);
                    self.expect(TokenKind::RBrace)?;
                }
                TokenKind::Output => {
                    self.advance();
                    self.expect(TokenKind::LBrace)?;
                    output = Some(self.parse_fields()?);
                    self.expect(TokenKind::RBrace)?;
                }
                TokenKind::Stage => {
                    stages.push(self.parse_pipeline_stage()?);
                }
                TokenKind::OnError => {
                    self.advance();
                    self.expect(TokenKind::LBrace)?;
                    let mut calls = Vec::new();
                    while self.peek_kind() != TokenKind::RBrace {
                        calls.push(self.parse_use_call()?);
                    }
                    self.expect(TokenKind::RBrace)?;
                    on_error = Some(calls);
                }
                TokenKind::Timeout => {
                    self.advance();
                    timeout = Some(self.expect_timeout_value()?);
                }
                _ => {
                    let s = self.peek_span();
                    let text = self.peek_text();
                    return Err(SkillSpecError::UnexpectedToken {
                        found: text,
                        expected: "input, output, stage, on_error, or timeout".to_string(),
                        span: s,
                    });
                }
            }
        }

        self.expect(TokenKind::RBrace)?;

        Ok(Pipeline {
            name,
            input,
            output,
            stages,
            on_error,
            timeout,
            span,
        })
    }

    fn parse_pipeline_stage(&mut self) -> Result<PipelineStage> {
        let span = self.peek_span();
        self.expect(TokenKind::Stage)?;
        let name = self.expect_ident()?;
        self.expect(TokenKind::LBrace)?;

        let mut requires = None;
        let mut use_call = None;

        while self.peek_kind() != TokenKind::RBrace {
            match self.peek_kind() {
                TokenKind::Requires => {
                    self.advance();
                    requires = Some(self.parse_dependency()?);
                }
                TokenKind::Use => {
                    use_call = Some(self.parse_use_call()?);
                }
                _ => {
                    let s = self.peek_span();
                    let text = self.peek_text();
                    return Err(SkillSpecError::UnexpectedToken {
                        found: text,
                        expected: "requires or use".to_string(),
                        span: s,
                    });
                }
            }
        }

        self.expect(TokenKind::RBrace)?;

        let use_call = use_call.ok_or_else(|| SkillSpecError::UnexpectedToken {
            found: "end of stage".to_string(),
            expected: "use call".to_string(),
            span,
        })?;

        Ok(PipelineStage {
            name,
            requires,
            use_call,
            span,
        })
    }

    // ── Orchestration ────────────────────────────────────────────────

    fn parse_orchestration(&mut self) -> Result<Orchestration> {
        let span = self.peek_span();
        self.expect(TokenKind::Orchestration)?;
        let name = self.expect_string_lit()?;
        self.expect(TokenKind::LBrace)?;

        let mut agents = Vec::new();
        let mut input = None;
        let mut output = None;
        let mut phases = Vec::new();
        let mut timeout = None;

        while self.peek_kind() != TokenKind::RBrace {
            match self.peek_kind() {
                TokenKind::Agents => {
                    self.advance();
                    self.expect(TokenKind::LBrace)?;
                    while self.peek_kind() != TokenKind::RBrace {
                        agents.push(self.parse_agent_decl()?);
                    }
                    self.expect(TokenKind::RBrace)?;
                }
                TokenKind::Input => {
                    self.advance();
                    self.expect(TokenKind::LBrace)?;
                    input = Some(self.parse_fields()?);
                    self.expect(TokenKind::RBrace)?;
                }
                TokenKind::Output => {
                    self.advance();
                    self.expect(TokenKind::LBrace)?;
                    output = Some(self.parse_fields()?);
                    self.expect(TokenKind::RBrace)?;
                }
                TokenKind::Phase => {
                    phases.push(self.parse_orchestrate_phase()?);
                }
                TokenKind::Timeout => {
                    self.advance();
                    timeout = Some(self.expect_timeout_value()?);
                }
                _ => {
                    let s = self.peek_span();
                    let text = self.peek_text();
                    return Err(SkillSpecError::UnexpectedToken {
                        found: text,
                        expected: "agents, input, output, phase, or timeout".to_string(),
                        span: s,
                    });
                }
            }
        }

        self.expect(TokenKind::RBrace)?;

        Ok(Orchestration {
            name,
            agents,
            input,
            output,
            phases,
            shared: None,
            rules: Vec::new(),
            timeout,
            span,
        })
    }

    fn parse_agent_decl(&mut self) -> Result<AgentDecl> {
        let agent_name = self.expect_ident()?;
        self.expect(TokenKind::Colon)?;
        // "agent" keyword — not a reserved keyword, will appear as an ident
        self.expect_specific_ident("agent")?;
        self.expect(TokenKind::LParen)?;

        let mut skill = String::new();
        let mut model = String::new();

        while self.peek_kind() != TokenKind::RParen {
            let key = self.expect_ident()?;
            self.expect(TokenKind::Colon)?;
            match key.as_str() {
                "skill" => {
                    skill = self.expect_string_lit()?;
                }
                "model" => {
                    model = self.expect_string_lit()?;
                }
                _ => {
                    let span = self.peek_span();
                    return Err(SkillSpecError::UnexpectedToken {
                        found: key,
                        expected: "skill or model".to_string(),
                        span,
                    });
                }
            }
            if self.peek_kind() == TokenKind::Comma {
                self.advance();
            }
        }
        self.expect(TokenKind::RParen)?;

        Ok(AgentDecl {
            name: agent_name,
            skill,
            model,
        })
    }

    fn parse_orchestrate_phase(&mut self) -> Result<OrchestratePhase> {
        let span = self.peek_span();
        self.expect(TokenKind::Phase)?;
        let name = self.expect_ident()?;
        self.expect(TokenKind::LBrace)?;

        let mut requires = None;
        let mut actions = Vec::new();
        let mut emit = None;

        while self.peek_kind() != TokenKind::RBrace {
            match self.peek_kind() {
                TokenKind::Requires => {
                    self.advance();
                    requires = Some(self.parse_dependency()?);
                }
                TokenKind::Emit => {
                    self.advance();
                    // "emit output from agent.result"
                    self.expect_specific_ident("output")?;
                    // consume "from"
                    self.expect(TokenKind::From)?;
                    // parse agent.result as an expression, then stringify
                    let expr = self.parse_primary_expr()?;
                    emit = Some(expr_to_source(&expr));
                }
                _ => {
                    // Try to parse agent.method(...) calls
                    // These look like: agent_name.method(args)
                    let action = self.parse_agent_action()?;
                    actions.push(action);
                }
            }
        }

        self.expect(TokenKind::RBrace)?;

        Ok(OrchestratePhase {
            name,
            requires,
            actions,
            emit,
            span,
        })
    }

    fn parse_agent_action(&mut self) -> Result<AgentAction> {
        let agent_name = self.expect_ident()?;
        self.expect(TokenKind::Dot)?;
        let method = self.expect_ident()?;
        self.expect(TokenKind::LParen)?;
        let args = self.parse_named_args()?;
        self.expect(TokenKind::RParen)?;

        Ok(AgentAction {
            agent_name,
            method,
            args,
        })
    }

    // ── Mixin ────────────────────────────────────────────────────────

    fn parse_mixin(&mut self) -> Result<Mixin> {
        let span = self.peek_span();
        self.expect(TokenKind::Mixin)?;
        let name = self.expect_ident()?;
        self.expect(TokenKind::LBrace)?;

        let mut steps = Vec::new();
        let mut contexts = Vec::new();

        while self.peek_kind() != TokenKind::RBrace {
            match self.peek_kind() {
                TokenKind::Step => {
                    steps.push(self.parse_step()?);
                }
                TokenKind::Context => {
                    contexts.push(self.parse_context_block()?);
                }
                _ => {
                    let s = self.peek_span();
                    let text = self.peek_text();
                    return Err(SkillSpecError::UnexpectedToken {
                        found: text,
                        expected: "step or context".to_string(),
                        span: s,
                    });
                }
            }
        }

        self.expect(TokenKind::RBrace)?;

        Ok(Mixin {
            name,
            steps,
            contexts,
            span,
        })
    }

    // ── Helper: timeout value ────────────────────────────────────────

    /// Parse a timeout value like "30m", "1h" — an integer immediately followed by
    /// a time-unit identifier.  The lexer tokenises these as two separate tokens
    /// (IntLit + Ident), so we glue them back together.
    fn expect_timeout_value(&mut self) -> Result<String> {
        let n = self.expect_int_lit()?;
        let unit = self.expect_ident()?;
        Ok(format!("{}{}", n, unit))
    }

    /// Accept either a float literal or an integer literal and return it as f64.
    fn expect_number_as_f64(&mut self) -> Result<f64> {
        match self.peek_kind() {
            TokenKind::FloatLit(f) => {
                self.advance();
                Ok(f)
            }
            TokenKind::IntLit(n) => {
                self.advance();
                Ok(n as f64)
            }
            _ => {
                let span = self.peek_span();
                let text = self.peek_text();
                Err(SkillSpecError::UnexpectedToken {
                    found: text,
                    expected: "number (int or float)".to_string(),
                    span,
                })
            }
        }
    }

    // ── Utility methods ───────────────────────────────────────────────

    fn at_end(&self) -> bool {
        self.pos >= self.tokens.len()
            || matches!(self.tokens[self.pos].kind, TokenKind::Eof)
    }

    fn peek_kind(&self) -> TokenKind {
        if self.pos < self.tokens.len() {
            self.tokens[self.pos].kind.clone()
        } else {
            TokenKind::Eof
        }
    }

    fn peek_text(&self) -> String {
        if self.pos < self.tokens.len() {
            self.tokens[self.pos].text.clone()
        } else {
            "EOF".to_string()
        }
    }

    fn peek_span(&self) -> Span {
        if self.pos < self.tokens.len() {
            self.tokens[self.pos].span
        } else {
            Span {
                start: 0,
                end: 0,
                line: 0,
                col: 0,
            }
        }
    }

    fn advance(&mut self) {
        if self.pos < self.tokens.len() {
            self.pos += 1;
        }
    }

    fn expect(&mut self, expected: TokenKind) -> Result<()> {
        let actual = self.peek_kind();
        if std::mem::discriminant(&actual) == std::mem::discriminant(&expected) {
            self.advance();
            Ok(())
        } else {
            let span = self.peek_span();
            let text = self.peek_text();
            Err(SkillSpecError::UnexpectedToken {
                found: text,
                expected: format!("{:?}", expected),
                span,
            })
        }
    }

    fn expect_ident(&mut self) -> Result<String> {
        // Accept genuine identifiers first.
        if let TokenKind::Ident(ref name) = self.peek_kind() {
            let n = name.clone();
            self.advance();
            return Ok(n);
        }
        // Also accept keyword tokens in identifier position (e.g. `message` as a field name,
        // `when` as a context-parameter name).  We use the raw source text so the string we
        // return matches what the programmer wrote.
        let text = self.peek_text();
        let is_keyword = matches!(
            self.peek_kind(),
            TokenKind::Skill
                | TokenKind::Input
                | TokenKind::Output
                | TokenKind::Body
                | TokenKind::Context
                | TokenKind::Step
                | TokenKind::Requires
                | TokenKind::When
                | TokenKind::Use
                | TokenKind::Let
                | TokenKind::Emit
                | TokenKind::Import
                | TokenKind::From
                | TokenKind::Type
                | TokenKind::Pre
                | TokenKind::Post
                | TokenKind::Assert
                | TokenKind::Message
                | TokenKind::OnError
                | TokenKind::AllSteps
                | TokenKind::Extends
                | TokenKind::StringType
                | TokenKind::IntType
                | TokenKind::FloatType
                | TokenKind::BoolType
                | TokenKind::Enum
                | TokenKind::Map
                // Phase 2 keywords — also valid in identifier position (e.g. field names)
                | TokenKind::Lazy
                | TokenKind::Ref
                | TokenKind::Summary
                | TokenKind::Index
                | TokenKind::Section
                | TokenKind::Load
                | TokenKind::Pipeline
                | TokenKind::Stage
                | TokenKind::Orchestration
                | TokenKind::Agents
                | TokenKind::Phase
                | TokenKind::Shared
                | TokenKind::Rules
                | TokenKind::Cancel
                | TokenKind::Timeout
                | TokenKind::Mixin
                | TokenKind::Include
                | TokenKind::Reasoning
                | TokenKind::Examples
                | TokenKind::Example
                | TokenKind::Note
                | TokenKind::Format
                | TokenKind::Reinforce
                | TokenKind::Every
                | TokenKind::On
                | TokenKind::Sampling
                | TokenKind::Persona
                | TokenKind::Tools
                | TokenKind::Require
                | TokenKind::Optional
                | TokenKind::Mcp
                | TokenKind::Tool
                | TokenKind::Allow
                | TokenKind::Deny
                | TokenKind::Permissions
                | TokenKind::If
                | TokenKind::Retry
                | TokenKind::Backoff
                // Phase 3 test keywords
                | TokenKind::Tests
                | TokenKind::Test
                | TokenKind::Given
                | TokenKind::Mock
                | TokenKind::Expect
                | TokenKind::Confidence
                | TokenKind::Runs
                | TokenKind::Snapshot
                | TokenKind::Compare
                | TokenKind::Equals
                | TokenKind::Contains
                | TokenKind::Matches
                | TokenKind::Resembles
                | TokenKind::Satisfies
                | TokenKind::Between
                | TokenKind::Unavailable
                | TokenKind::Failing
                | TokenKind::Slow
                // Package management keywords
                | TokenKind::Package
                | TokenKind::Version
                | TokenKind::Description
                | TokenKind::Exports
        );
        if is_keyword && !text.is_empty() {
            self.advance();
            return Ok(text);
        }
        let span = self.peek_span();
        Err(SkillSpecError::UnexpectedToken {
            found: text,
            expected: "identifier".to_string(),
            span,
        })
    }

    fn expect_specific_ident(&mut self, expected: &str) -> Result<()> {
        if let TokenKind::Ident(ref name) = self.peek_kind() {
            if name == expected {
                self.advance();
                return Ok(());
            }
        }
        // Handle keyword tokens that match the expected string
        if expected == "output" && matches!(self.peek_kind(), TokenKind::Output) {
            self.advance();
            return Ok(());
        }
        let span = self.peek_span();
        let text = self.peek_text();
        Err(SkillSpecError::UnexpectedToken {
            found: text,
            expected: format!("'{}'", expected),
            span,
        })
    }

    fn expect_string_lit(&mut self) -> Result<String> {
        if let TokenKind::StringLit(ref s) = self.peek_kind() {
            let val = s.clone();
            self.advance();
            Ok(val)
        } else {
            let span = self.peek_span();
            let text = self.peek_text();
            Err(SkillSpecError::UnexpectedToken {
                found: text,
                expected: "string literal".to_string(),
                span,
            })
        }
    }

    fn expect_int_lit(&mut self) -> Result<i64> {
        if let TokenKind::IntLit(n) = self.peek_kind() {
            self.advance();
            Ok(n)
        } else {
            let span = self.peek_span();
            let text = self.peek_text();
            Err(SkillSpecError::UnexpectedToken {
                found: text,
                expected: "integer literal".to_string(),
                span,
            })
        }
    }

    /// Check if the current token is the identifier "where" (used in quantifier assertions).
    fn is_where_keyword(&self) -> bool {
        matches!(self.peek_kind(), TokenKind::Ident(ref s) if s == "where")
    }

    fn expect_float_lit(&mut self) -> Result<f64> {
        if let TokenKind::FloatLit(f) = self.peek_kind() {
            self.advance();
            Ok(f)
        } else {
            let span = self.peek_span();
            let text = self.peek_text();
            Err(SkillSpecError::UnexpectedToken {
                found: text,
                expected: "float literal".to_string(),
                span,
            })
        }
    }
}

/// Convert an expression back to its source representation.
/// Used for stringifying orchestration emit targets (e.g. `lead.result`).
fn expr_to_source(expr: &Expr) -> String {
    match expr {
        Expr::Ident(name) => name.clone(),
        Expr::FieldAccess(obj, field) => format!("{}.{}", expr_to_source(obj), field),
        Expr::StringLit(s) => format!("\"{}\"", s),
        Expr::IntLit(n) => n.to_string(),
        Expr::FloatLit(f) => f.to_string(),
        Expr::BoolLit(b) => b.to_string(),
        Expr::ArrayLit(items) => {
            let parts: Vec<String> = items.iter().map(|e| expr_to_source(e)).collect();
            format!("[{}]", parts.join(", "))
        }
        Expr::BinOp(lhs, op, rhs) => {
            let op_str = match op {
                BinOp::Eq => "==",
                BinOp::NotEq => "!=",
                BinOp::Lt => "<",
                BinOp::Gt => ">",
                BinOp::LtEq => "<=",
                BinOp::GtEq => ">=",
                BinOp::In => "in",
                BinOp::And => "&&",
                BinOp::Or => "||",
            };
            format!("{} {} {}", expr_to_source(lhs), op_str, expr_to_source(rhs))
        }
        Expr::Not(inner) => format!("!{}", expr_to_source(inner)),
        Expr::FnCall(name, args) => {
            let parts: Vec<String> = args.iter()
                .map(|(k, v)| format!("{}: {}", k, expr_to_source(v)))
                .collect();
            format!("{}({})", name, parts.join(", "))
        }
        Expr::Interpolated(s) => format!("`{}`", s),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::Lexer;

    fn parse(input: &str) -> SourceFile {
        let lexer = Lexer::new(input);
        let tokens = lexer.tokenize().unwrap();
        let mut parser = Parser::new(tokens);
        parser.parse().unwrap()
    }

    #[test]
    fn minimal_skill() {
        let file = parse(r#"skill "hello" { context { "Greet the user warmly." } }"#);
        assert_eq!(file.skills.len(), 1);
        assert_eq!(file.skills[0].name, "hello");
        assert_eq!(file.skills[0].body.contexts.len(), 1);
    }

    #[test]
    fn skill_with_input_output() {
        let file = parse(r#"
            skill "review" {
                input {
                    files: string[]
                    severity: enum("high", "medium", "low") = "medium"
                    reviewer?: string
                }
                output {
                    findings: Finding[]
                    summary: string
                }
                body {
                    context { "Review the code." }
                }
            }
        "#);
        let skill = &file.skills[0];
        let input = skill.input.as_ref().unwrap();
        assert_eq!(input.len(), 3);
        assert_eq!(input[0].name, "files");
        assert!(!input[0].optional);
        assert_eq!(input[1].name, "severity");
        assert!(input[1].default.is_some());
        assert_eq!(input[2].name, "reviewer");
        assert!(input[2].optional);
    }

    #[test]
    fn skill_with_steps() {
        let file = parse(r#"
            skill "test" {
                body {
                    step analyze {
                        use static_analysis(files: input.files)
                    }
                    step review {
                        requires analyze
                        context { "Review code." }
                    }
                    step finish {
                        requires analyze & review
                        emit output
                    }
                }
            }
        "#);
        let body = &file.skills[0].body;
        assert_eq!(body.steps.len(), 3);
        assert_eq!(body.steps[0].name, "analyze");
        assert!(body.steps[0].use_call.is_some());
        assert_eq!(body.steps[1].name, "review");
        assert!(matches!(body.steps[1].requires, Some(Dependency::Single(ref s)) if s == "analyze"));
        assert!(body.steps[2].emit);
    }

    #[test]
    fn type_definition() {
        let file = parse(r#"
            type Finding {
                file: string
                line: int
                severity: enum("critical", "high")
                suggestion?: string
            }
            skill "x" { context { "ok" } }
        "#);
        assert_eq!(file.type_defs.len(), 1);
        assert_eq!(file.type_defs[0].name, "Finding");
        assert_eq!(file.type_defs[0].fields.len(), 4);
        assert!(file.type_defs[0].fields[3].optional);
    }

    #[test]
    fn import_statement() {
        let file = parse(r#"
            import { Finding, Severity } from "@types/review"
            skill "x" { context { "ok" } }
        "#);
        assert_eq!(file.imports.len(), 1);
        assert_eq!(file.imports[0].symbols, vec!["Finding", "Severity"]);
        assert_eq!(file.imports[0].path, "@types/review");
    }

    #[test]
    fn context_with_params() {
        let file = parse(r#"
            skill "x" {
                body {
                    context(priority: 80, decay: 0.5) {
                        """
                        You are an expert reviewer.
                        Focus on security issues.
                        """
                    }
                }
            }
        "#);
        let ctx = &file.skills[0].body.contexts[0];
        assert_eq!(ctx.priority, Some(80));
        assert_eq!(ctx.decay, Some(0.5));
    }

    #[test]
    fn any_dependency() {
        let file = parse(r#"
            skill "x" {
                body {
                    step a { context { "a" } }
                    step b { context { "b" } }
                    step c {
                        requires a | b
                        context { "c" }
                    }
                }
            }
        "#);
        let step = &file.skills[0].body.steps[2];
        assert!(matches!(step.requires, Some(Dependency::Any(_))));
    }

    #[test]
    fn pre_post_contracts() {
        let file = parse(r#"
            skill "deploy" {
                input { branch: string }
                pre {
                    assert input.branch != "main" message "Don't deploy main"
                }
                post {
                    assert output.status == "success" message "Must succeed"
                }
                body { context { "Deploy." } }
            }
        "#);
        assert_eq!(file.skills[0].pre.len(), 1);
        assert_eq!(file.skills[0].post.len(), 1);
        assert_eq!(file.skills[0].pre[0].message, "Don't deploy main");
    }

    // ── Phase 2 tests ────────────────────────────────────────────────

    #[test]
    fn lazy_context_with_ref() {
        let file = parse(r#"
            skill "x" {
                body {
                    lazy context "docs" (priority: 50) {
                        summary "API docs."
                        ref "./api.md"
                    }
                    step main {
                        load "docs"
                        context { "Use the docs." }
                    }
                }
            }
        "#);
        assert_eq!(file.skills[0].body.lazy_contexts.len(), 1);
        assert_eq!(file.skills[0].body.lazy_contexts[0].name, "docs");
        assert_eq!(file.skills[0].body.steps[0].loads, vec!["docs"]);
    }

    #[test]
    fn tools_block() {
        let file = parse(r#"
            skill "x" {
                tools {
                    require Bash
                    require mcp("github") {
                        pr_diff(repo: string, pr: int) -> string
                    }
                    optional mcp("slack") {
                        send_message(channel: string, text: string) -> void
                    }
                }
                body { context { "ok" } }
            }
        "#);
        let tools = file.skills[0].tools.as_ref().unwrap();
        assert_eq!(tools.required.len(), 2);
        assert_eq!(tools.optional.len(), 1);
    }

    #[test]
    fn prompt_directives() {
        let file = parse(r#"
            skill "x" {
                body {
                    reasoning extended
                    persona { "You are an expert." }
                    sampling {
                        temperature: 0.2
                    }
                    context { "Do stuff." }
                }
            }
        "#);
        let directives = &file.skills[0].body.directives;
        assert_eq!(directives.reasoning.as_deref(), Some("extended"));
        assert!(directives.persona.is_some());
        assert!(directives.sampling.is_some());
    }

    #[test]
    fn pipeline_construct() {
        let file = parse(r#"
            pipeline "review" {
                input { repo: string }
                output { report: string }
                stage lint { use linter(files: input.files) }
                stage review {
                    requires lint
                    use code_review(results: lint.result)
                }
                timeout 30m
            }
        "#);
        assert_eq!(file.pipelines.len(), 1);
        assert_eq!(file.pipelines[0].stages.len(), 2);
        assert_eq!(file.pipelines[0].timeout.as_deref(), Some("30m"));
    }

    #[test]
    fn orchestration_construct() {
        let file = parse(r#"
            orchestration "collab" {
                agents {
                    reviewer: agent(skill: "code-review", model: "opus")
                }
                input { pr_url: string }
                output { decision: string }
                phase review {
                    reviewer.run(files: input.files)
                }
                timeout 1h
            }
        "#);
        assert_eq!(file.orchestrations.len(), 1);
        assert_eq!(file.orchestrations[0].agents.len(), 1);
        assert_eq!(file.orchestrations[0].phases.len(), 1);
    }

    // ── Phase 3 test block tests ───────────────────────────────────

    #[test]
    fn test_block_parsing() {
        let file = parse(r#"
            skill "x" {
                body { context { "ok" } }
                tests {
                    test "basic" {
                        given {
                            files: ["test.py"]
                        }
                        expect {
                            output.status: equals("success")
                        }
                    }
                }
            }
        "#);
        assert_eq!(file.skills[0].tests.len(), 1);
        assert_eq!(file.skills[0].tests[0].name, "basic");
    }

    #[test]
    fn test_with_mocks_and_confidence() {
        let file = parse(r#"
            skill "x" {
                body { context { "ok" } }
                tests {
                    test "complex" {
                        given {
                            query: "test input"
                        }
                        mock tools.mcp.slack: unavailable
                        expect {
                            output.result: matches(".*test.*")
                        }
                        confidence 0.9
                        runs 10
                    }
                }
            }
        "#);
        let test = &file.skills[0].tests[0];
        assert_eq!(test.name, "complex");
        assert_eq!(test.mocks.len(), 1);
        assert_eq!(test.confidence, Some(0.9));
        assert_eq!(test.runs, Some(10));
    }

    #[test]
    fn test_with_mock_responses() {
        let file = parse(r#"
            skill "x" {
                body { context { "ok" } }
                tests {
                    test "mock responses" {
                        mock tools.mcp.github {
                            pr_diff(repo: "org/app", pr: 42) -> "SELECT * FROM users"
                        }
                        expect {
                            output.status: equals("done")
                        }
                    }
                }
            }
        "#);
        let test = &file.skills[0].tests[0];
        assert_eq!(test.mocks.len(), 1);
        assert_eq!(test.mocks[0].tool_path, "tools.mcp.github");
        assert!(matches!(test.mocks[0].mock_type, MockType::Responses(_)));
    }

    #[test]
    fn test_multiple_expectations() {
        let file = parse(r#"
            skill "x" {
                body { context { "ok" } }
                tests {
                    test "multi expect" {
                        expect {
                            output.count: >= 1
                            output.score: between(0.5, 1.0)
                            output.name: matches(".*hello.*")
                        }
                    }
                }
            }
        "#);
        let test = &file.skills[0].tests[0];
        assert_eq!(test.expectations.len(), 3);
        assert_eq!(test.expectations[0].path, "output.count");
        assert_eq!(test.expectations[1].path, "output.score");
        assert_eq!(test.expectations[2].path, "output.name");
    }

    #[test]
    fn test_multiple_test_blocks() {
        let file = parse(r#"
            skill "x" {
                body { context { "ok" } }
                tests {
                    test "first" {
                        expect { output.a: equals("x") }
                    }
                    test "second" {
                        expect { output.b: equals("y") }
                    }
                }
            }
        "#);
        assert_eq!(file.skills[0].tests.len(), 2);
        assert_eq!(file.skills[0].tests[0].name, "first");
        assert_eq!(file.skills[0].tests[1].name, "second");
    }

    #[test]
    fn logical_and_in_when() {
        let file = parse(r#"
            skill "x" {
                body {
                    step a {
                        when input.focus == "types" && input.mode == "strict"
                        context { "Both conditions." }
                    }
                }
            }
        "#);
        let step = &file.skills[0].body.steps[0];
        assert!(step.when.is_some());
        if let Some(Expr::BinOp(_, BinOp::And, _)) = &step.when {
            // correct
        } else {
            panic!("Expected BinOp::And, got {:?}", step.when);
        }
    }

    #[test]
    fn logical_or_in_when() {
        let file = parse(r#"
            skill "x" {
                body {
                    step a {
                        when input.focus == "types" || input.focus == "all"
                        context { "Either condition." }
                    }
                }
            }
        "#);
        let step = &file.skills[0].body.steps[0];
        assert!(step.when.is_some());
        if let Some(Expr::BinOp(_, BinOp::Or, _)) = &step.when {
            // correct
        } else {
            panic!("Expected BinOp::Or, got {:?}", step.when);
        }
    }

    #[test]
    fn quantifier_assertions() {
        let file = parse(r#"
            skill "x" {
                body { context { "ok" } }
                tests {
                    test "quantifiers" {
                        given { files: ["test.py"] }
                        expect {
                            output.findings: contains(where: .severity == "high")
                            output.items: none(where: .status == "failed")
                        }
                    }
                }
            }
        "#);
        let test = &file.skills[0].tests[0];
        assert_eq!(test.expectations.len(), 2);
    }

    #[test]
    fn mixin_and_include() {
        let file = parse(r#"
            mixin logging {
                step log_start {
                    context { "Starting." }
                }
            }
            skill "deploy" {
                include logging
                body {
                    context { "Deploy." }
                }
            }
        "#);
        assert_eq!(file.mixins.len(), 1);
        assert_eq!(file.mixins[0].name, "logging");
        assert_eq!(file.skills[0].includes, vec!["logging"]);
    }

    // ── Package tests ────────────────────────────────────────────────

    #[test]
    fn package_declaration() {
        let file = parse(r#"
            package "@tools/review" {
                version = "1.0.0"
                description = "Code review toolkit"
                exports {
                    code_review
                    Finding
                }
            }
            type Finding {
                file: string
                severity: string
            }
            skill "code-review" {
                body { context { "Review code." } }
            }
        "#);
        assert_eq!(file.packages.len(), 1);
        assert_eq!(file.packages[0].name, "@tools/review");
        assert_eq!(file.packages[0].version, "1.0.0");
        assert_eq!(file.packages[0].description, "Code review toolkit");
        assert_eq!(file.packages[0].exports, vec!["code_review", "Finding"]);
    }

    #[test]
    fn package_with_no_exports() {
        let file = parse(r#"
            package "@tools/minimal" {
                version = "0.1.0"
                description = "Minimal package"
                exports {}
            }
        "#);
        assert_eq!(file.packages.len(), 1);
        assert_eq!(file.packages[0].exports.len(), 0);
    }

    #[test]
    fn package_minimal_fields() {
        let file = parse(r#"
            package "@test/pkg" {
                version = "2.0.0"
                description = "Test"
                exports { my_skill }
            }
        "#);
        let pkg = &file.packages[0];
        assert_eq!(pkg.name, "@test/pkg");
        assert_eq!(pkg.version, "2.0.0");
        assert_eq!(pkg.exports, vec!["my_skill"]);
    }

    #[test]
    fn multiple_packages() {
        let file = parse(r#"
            package "@tools/a" {
                version = "1.0.0"
                description = "Package A"
                exports { skill_a }
            }
            package "@tools/b" {
                version = "2.0.0"
                description = "Package B"
                exports { skill_b }
            }
        "#);
        assert_eq!(file.packages.len(), 2);
        assert_eq!(file.packages[0].name, "@tools/a");
        assert_eq!(file.packages[1].name, "@tools/b");
    }

    // ── Negative parser tests ────────────────────────────────────────

    #[test]
    fn error_on_missing_skill_name() {
        let input = r#"skill { context { "no name" } }"#;
        let tokens = Lexer::new(input).tokenize().unwrap();
        let result = Parser::new(tokens).parse();
        assert!(result.is_err());
    }

    #[test]
    fn error_on_unclosed_brace() {
        let input = r#"skill "broken" { context { "unclosed" }"#;
        let tokens = Lexer::new(input).tokenize().unwrap();
        let result = Parser::new(tokens).parse();
        assert!(result.is_err());
    }

    #[test]
    fn error_on_unknown_section() {
        let input = r#"skill "x" { foobar { } body { context { "ok" } } }"#;
        let tokens = Lexer::new(input).tokenize().unwrap();
        let result = Parser::new(tokens).parse();
        assert!(result.is_err());
    }

    #[test]
    fn error_on_duplicate_emit() {
        // This should parse fine but fail the checker
        let input = r#"
            skill "x" {
                body {
                    step a { emit output context { "a" } }
                    step b { emit output context { "b" } }
                }
            }
        "#;
        let tokens = Lexer::new(input).tokenize().unwrap();
        let ast = Parser::new(tokens).parse();
        assert!(ast.is_ok()); // parser accepts it — checker catches it
    }
}
