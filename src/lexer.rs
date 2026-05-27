use crate::error::{Result, SkillSpecError};
use crate::token::{Span, Token, TokenKind};

pub struct Lexer {
    chars: Vec<char>,
    pos: usize,
    line: usize,
    col: usize,
}

impl Lexer {
    pub fn new(input: &str) -> Self {
        Lexer {
            chars: input.chars().collect(),
            pos: 0,
            line: 1,
            col: 1,
        }
    }

    pub fn tokenize(mut self) -> Result<Vec<Token>> {
        let mut tokens = Vec::new();
        loop {
            self.skip_whitespace();
            if self.pos >= self.chars.len() {
                let span = self.current_span(self.pos);
                tokens.push(Token {
                    kind: TokenKind::Eof,
                    span,
                    text: String::new(),
                });
                break;
            }
            // Skip line comments
            if self.peek() == Some('/') && self.peek_at(1) == Some('/') {
                self.skip_line_comment();
                continue;
            }
            let token = self.next_token()?;
            tokens.push(token);
        }
        Ok(tokens)
    }

    fn next_token(&mut self) -> Result<Token> {
        let start_pos = self.pos;
        let start_line = self.line;
        let start_col = self.col;

        let ch = match self.peek() {
            Some(c) => c,
            None => {
                let span = self.current_span(start_pos);
                return Ok(Token {
                    kind: TokenKind::Eof,
                    span,
                    text: String::new(),
                });
            }
        };

        // Triple-quoted string
        if ch == '"' && self.peek_at(1) == Some('"') && self.peek_at(2) == Some('"') {
            return self.lex_triple_string(start_pos, start_line, start_col);
        }

        // Regular string
        if ch == '"' {
            return self.lex_string(start_pos, start_line, start_col);
        }

        // Numbers: digit or '-' followed by digit
        if ch.is_ascii_digit() || (ch == '-' && self.peek_at(1).is_some_and(|c| c.is_ascii_digit()))
        {
            return self.lex_number(start_pos, start_line, start_col);
        }

        // Identifiers, keywords, @-prefixed identifiers
        if ch.is_alphabetic() || ch == '_' || ch == '@' {
            return self.lex_ident_or_keyword(start_pos, start_line, start_col);
        }

        // Operators and punctuation
        self.advance();
        let span = Span {
            start: start_pos,
            end: self.pos,
            line: start_line,
            col: start_col,
        };
        let kind = match ch {
            '{' => TokenKind::LBrace,
            '}' => TokenKind::RBrace,
            '(' => TokenKind::LParen,
            ')' => TokenKind::RParen,
            '[' => TokenKind::LBracket,
            ']' => TokenKind::RBracket,
            ':' => TokenKind::Colon,
            ',' => TokenKind::Comma,
            '.' => TokenKind::Dot,
            '?' => TokenKind::Question,
            '&' => {
                if self.peek() == Some('&') {
                    self.advance();
                    TokenKind::AmpAmp
                } else {
                    TokenKind::Amp
                }
            }
            '|' => {
                if self.peek() == Some('|') {
                    self.advance();
                    TokenKind::PipePipe
                } else {
                    TokenKind::Pipe
                }
            }
            '=' => {
                if self.peek() == Some('=') {
                    self.advance();
                    TokenKind::EqEq
                } else {
                    TokenKind::Eq
                }
            }
            '!' => {
                if self.peek() == Some('=') {
                    self.advance();
                    TokenKind::NotEq
                } else {
                    TokenKind::Bang
                }
            }
            '>' => {
                if self.peek() == Some('=') {
                    self.advance();
                    TokenKind::GtEq
                } else {
                    TokenKind::Gt
                }
            }
            '<' => {
                if self.peek() == Some('=') {
                    self.advance();
                    TokenKind::LtEq
                } else {
                    TokenKind::Lt
                }
            }
            '-' => {
                if self.peek() == Some('>') {
                    self.advance();
                    TokenKind::Arrow
                } else {
                    return Err(SkillSpecError::LexerError {
                        message: format!("Unexpected character '{}'", ch),
                        span,
                    });
                }
            }
            other => {
                return Err(SkillSpecError::LexerError {
                    message: format!("Unexpected character '{}'", other),
                    span,
                });
            }
        };

        let end_pos = self.pos;
        let text = self.chars[start_pos..end_pos].iter().collect::<String>();
        Ok(Token {
            kind,
            span: Span {
                start: start_pos,
                end: end_pos,
                line: start_line,
                col: start_col,
            },
            text,
        })
    }

    fn lex_string(
        &mut self,
        start_pos: usize,
        start_line: usize,
        start_col: usize,
    ) -> Result<Token> {
        // consume opening '"'
        self.advance();
        let mut s = String::new();
        loop {
            match self.peek() {
                None | Some('\n') => {
                    let span = Span {
                        start: start_pos,
                        end: self.pos,
                        line: start_line,
                        col: start_col,
                    };
                    return Err(SkillSpecError::LexerError {
                        message: "Unterminated string literal".to_string(),
                        span,
                    });
                }
                Some('"') => {
                    self.advance();
                    break;
                }
                Some('\\') => {
                    self.advance();
                    match self.peek() {
                        Some('n') => {
                            self.advance();
                            s.push('\n');
                        }
                        Some('t') => {
                            self.advance();
                            s.push('\t');
                        }
                        Some('r') => {
                            self.advance();
                            s.push('\r');
                        }
                        Some('"') => {
                            self.advance();
                            s.push('"');
                        }
                        Some('\\') => {
                            self.advance();
                            s.push('\\');
                        }
                        Some(c) => {
                            self.advance();
                            s.push('\\');
                            s.push(c);
                        }
                        None => {
                            let span = Span {
                                start: start_pos,
                                end: self.pos,
                                line: start_line,
                                col: start_col,
                            };
                            return Err(SkillSpecError::LexerError {
                                message: "Unterminated string escape".to_string(),
                                span,
                            });
                        }
                    }
                }
                Some(c) => {
                    self.advance();
                    s.push(c);
                }
            }
        }
        let end_pos = self.pos;
        let text = self.chars[start_pos..end_pos].iter().collect::<String>();
        Ok(Token {
            kind: TokenKind::StringLit(s),
            span: Span {
                start: start_pos,
                end: end_pos,
                line: start_line,
                col: start_col,
            },
            text,
        })
    }

    fn lex_triple_string(
        &mut self,
        start_pos: usize,
        start_line: usize,
        start_col: usize,
    ) -> Result<Token> {
        // consume opening '"""'
        self.advance();
        self.advance();
        self.advance();

        let mut s = String::new();

        // Skip a leading newline immediately after opening quotes
        if self.peek() == Some('\n') {
            self.advance();
        }

        loop {
            // Check for closing """
            if self.peek() == Some('"')
                && self.peek_at(1) == Some('"')
                && self.peek_at(2) == Some('"')
            {
                self.advance();
                self.advance();
                self.advance();
                break;
            }
            match self.peek() {
                None => {
                    let span = Span {
                        start: start_pos,
                        end: self.pos,
                        line: start_line,
                        col: start_col,
                    };
                    return Err(SkillSpecError::LexerError {
                        message: "Unterminated triple-quoted string".to_string(),
                        span,
                    });
                }
                Some(c) => {
                    self.advance();
                    s.push(c);
                }
            }
        }

        // Trim trailing whitespace before closing quotes
        let trimmed = s.trim_end().to_string();

        let end_pos = self.pos;
        let text = self.chars[start_pos..end_pos].iter().collect::<String>();
        Ok(Token {
            kind: TokenKind::TripleString(trimmed),
            span: Span {
                start: start_pos,
                end: end_pos,
                line: start_line,
                col: start_col,
            },
            text,
        })
    }

    fn lex_number(
        &mut self,
        start_pos: usize,
        start_line: usize,
        start_col: usize,
    ) -> Result<Token> {
        let mut s = String::new();

        // Optional leading minus
        if self.peek() == Some('-') {
            s.push('-');
            self.advance();
        }

        // Integer digits
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() {
                s.push(c);
                self.advance();
            } else {
                break;
            }
        }

        // Optional fractional part
        let is_float =
            self.peek() == Some('.') && self.peek_at(1).is_some_and(|c| c.is_ascii_digit());
        if is_float {
            s.push('.');
            self.advance(); // consume '.'
            while let Some(c) = self.peek() {
                if c.is_ascii_digit() {
                    s.push(c);
                    self.advance();
                } else {
                    break;
                }
            }
        }

        let end_pos = self.pos;
        let text = self.chars[start_pos..end_pos].iter().collect::<String>();
        let span = Span {
            start: start_pos,
            end: end_pos,
            line: start_line,
            col: start_col,
        };

        if is_float {
            match s.parse::<f64>() {
                Ok(f) => Ok(Token {
                    kind: TokenKind::FloatLit(f),
                    span,
                    text,
                }),
                Err(_) => Err(SkillSpecError::LexerError {
                    message: format!("Invalid float '{}'", s),
                    span,
                }),
            }
        } else {
            match s.parse::<i64>() {
                Ok(i) => Ok(Token {
                    kind: TokenKind::IntLit(i),
                    span,
                    text,
                }),
                Err(_) => Err(SkillSpecError::LexerError {
                    message: format!("Invalid integer '{}'", s),
                    span,
                }),
            }
        }
    }

    fn lex_ident_or_keyword(
        &mut self,
        start_pos: usize,
        start_line: usize,
        start_col: usize,
    ) -> Result<Token> {
        let mut s = String::new();

        // Allow '@' as first char
        if self.peek() == Some('@') {
            s.push('@');
            self.advance();
        }

        while let Some(c) = self.peek() {
            if c.is_alphanumeric() || c == '_' {
                s.push(c);
                self.advance();
            } else {
                break;
            }
        }

        let end_pos = self.pos;
        let text = self.chars[start_pos..end_pos].iter().collect::<String>();
        let span = Span {
            start: start_pos,
            end: end_pos,
            line: start_line,
            col: start_col,
        };

        let kind = match s.as_str() {
            "skill" => TokenKind::Skill,
            "input" => TokenKind::Input,
            "output" => TokenKind::Output,
            "body" => TokenKind::Body,
            "context" => TokenKind::Context,
            "step" => TokenKind::Step,
            "requires" => TokenKind::Requires,
            "when" => TokenKind::When,
            "use" => TokenKind::Use,
            "let" => TokenKind::Let,
            "emit" => TokenKind::Emit,
            "import" => TokenKind::Import,
            "from" => TokenKind::From,
            "type" => TokenKind::Type,
            "pre" => TokenKind::Pre,
            "post" => TokenKind::Post,
            "assert" => TokenKind::Assert,
            "message" => TokenKind::Message,
            "on_error" => TokenKind::OnError,
            "all_steps" => TokenKind::AllSteps,
            "extends" => TokenKind::Extends,
            // Lazy context
            "lazy" => TokenKind::Lazy,
            "ref" => TokenKind::Ref,
            "summary" => TokenKind::Summary,
            "index" => TokenKind::Index,
            "section" => TokenKind::Section,
            "load" => TokenKind::Load,
            // Pipeline / Orchestration
            "pipeline" => TokenKind::Pipeline,
            "stage" => TokenKind::Stage,
            "orchestration" => TokenKind::Orchestration,
            "agents" => TokenKind::Agents,
            "phase" => TokenKind::Phase,
            "shared" => TokenKind::Shared,
            "rules" => TokenKind::Rules,
            "cancel" => TokenKind::Cancel,
            "timeout" => TokenKind::Timeout,
            // Mixin / composition
            "mixin" => TokenKind::Mixin,
            "include" => TokenKind::Include,
            // Package management
            "package" => TokenKind::Package,
            "version" => TokenKind::Version,
            "description" => TokenKind::Description,
            "exports" => TokenKind::Exports,
            // Prompt directives
            "reasoning" => TokenKind::Reasoning,
            "examples" => TokenKind::Examples,
            "example" => TokenKind::Example,
            "note" => TokenKind::Note,
            "format" => TokenKind::Format,
            "reinforce" => TokenKind::Reinforce,
            "every" => TokenKind::Every,
            "on" => TokenKind::On,
            "sampling" => TokenKind::Sampling,
            "persona" => TokenKind::Persona,
            // Tools / permissions
            "tools" => TokenKind::Tools,
            "require" => TokenKind::Require,
            "optional" => TokenKind::Optional,
            "mcp" => TokenKind::Mcp,
            "tool" => TokenKind::Tool,
            "allow" => TokenKind::Allow,
            "deny" => TokenKind::Deny,
            "permissions" => TokenKind::Permissions,
            // Test framework
            "tests" => TokenKind::Tests,
            "test" => TokenKind::Test,
            "given" => TokenKind::Given,
            "mock" => TokenKind::Mock,
            "expect" => TokenKind::Expect,
            "confidence" => TokenKind::Confidence,
            "runs" => TokenKind::Runs,
            "snapshot" => TokenKind::Snapshot,
            "compare" => TokenKind::Compare,
            "equals" => TokenKind::Equals,
            "contains" => TokenKind::Contains,
            "matches" => TokenKind::Matches,
            "resembles" => TokenKind::Resembles,
            "satisfies" => TokenKind::Satisfies,
            "between" => TokenKind::Between,
            "unavailable" => TokenKind::Unavailable,
            "failing" => TokenKind::Failing,
            "slow" => TokenKind::Slow,
            // Observability
            "observe" => TokenKind::Observe,
            "emit_event" => TokenKind::EmitEvent,
            "metric" => TokenKind::Metric,
            // Control flow
            "if" => TokenKind::If,
            "retry" => TokenKind::Retry,
            "backoff" => TokenKind::Backoff,
            // Primitives
            "string" => TokenKind::StringType,
            "int" => TokenKind::IntType,
            "float" => TokenKind::FloatType,
            "bool" => TokenKind::BoolType,
            "enum" => TokenKind::Enum,
            "map" => TokenKind::Map,
            "true" => TokenKind::BoolLit(true),
            "false" => TokenKind::BoolLit(false),
            "in" => TokenKind::Ident("in".into()),
            _ => TokenKind::Ident(s),
        };

        Ok(Token { kind, span, text })
    }

    fn skip_whitespace(&mut self) {
        while let Some(c) = self.peek() {
            if c.is_whitespace() {
                self.advance();
            } else {
                break;
            }
        }
    }

    fn skip_line_comment(&mut self) {
        // consume '//'
        self.advance();
        self.advance();
        while let Some(c) = self.peek() {
            if c == '\n' {
                break;
            }
            self.advance();
        }
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn peek_at(&self, offset: usize) -> Option<char> {
        self.chars.get(self.pos + offset).copied()
    }

    fn advance(&mut self) -> Option<char> {
        let ch = self.chars.get(self.pos).copied();
        if let Some(c) = ch {
            self.pos += 1;
            if c == '\n' {
                self.line += 1;
                self.col = 1;
            } else {
                self.col += 1;
            }
        }
        ch
    }

    fn current_span(&self, start: usize) -> Span {
        Span {
            start,
            end: self.pos,
            line: self.line,
            col: self.col,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lex(input: &str) -> Vec<TokenKind> {
        let lexer = Lexer::new(input);
        lexer
            .tokenize()
            .unwrap()
            .into_iter()
            .map(|t| t.kind)
            .collect()
    }

    #[test]
    fn minimal_skill() {
        let tokens = lex(r#"skill "hello" { context { "Greet the user." } }"#);
        assert_eq!(tokens[0], TokenKind::Skill);
        assert_eq!(tokens[1], TokenKind::StringLit("hello".into()));
        assert_eq!(tokens[2], TokenKind::LBrace);
        assert_eq!(tokens[3], TokenKind::Context);
    }

    #[test]
    fn triple_string() {
        let tokens = lex("context { \"\"\"hello world\"\"\" }");
        assert!(matches!(tokens[2], TokenKind::TripleString(_)));
        if let TokenKind::TripleString(s) = &tokens[2] {
            assert_eq!(s, "hello world");
        }
    }

    #[test]
    fn keywords_and_idents() {
        let tokens = lex("step analyze { requires build }");
        assert_eq!(tokens[0], TokenKind::Step);
        assert_eq!(tokens[1], TokenKind::Ident("analyze".into()));
        assert_eq!(tokens[3], TokenKind::Requires);
        assert_eq!(tokens[4], TokenKind::Ident("build".into()));
    }

    #[test]
    fn numbers() {
        let tokens = lex("priority: 80 decay: 0.5");
        assert_eq!(tokens[0], TokenKind::Ident("priority".into()));
        assert_eq!(tokens[2], TokenKind::IntLit(80));
        assert_eq!(tokens[5], TokenKind::FloatLit(0.5));
    }

    #[test]
    fn operators() {
        let tokens = lex("== != >= <= > <");
        assert_eq!(tokens[0], TokenKind::EqEq);
        assert_eq!(tokens[1], TokenKind::NotEq);
        assert_eq!(tokens[2], TokenKind::GtEq);
        assert_eq!(tokens[3], TokenKind::LtEq);
        assert_eq!(tokens[4], TokenKind::Gt);
        assert_eq!(tokens[5], TokenKind::Lt);
    }

    #[test]
    fn comments_skipped() {
        let tokens = lex("skill // this is a comment\n\"test\"");
        assert_eq!(tokens[0], TokenKind::Skill);
        assert_eq!(tokens[1], TokenKind::StringLit("test".into()));
    }

    #[test]
    fn import_statement() {
        let tokens = lex(r#"import { Finding } from "@types/review""#);
        assert_eq!(tokens[0], TokenKind::Import);
        assert_eq!(tokens[1], TokenKind::LBrace);
        assert_eq!(tokens[2], TokenKind::Ident("Finding".into()));
        assert_eq!(tokens[3], TokenKind::RBrace);
        assert_eq!(tokens[4], TokenKind::From);
    }

    #[test]
    fn logical_operators() {
        let tokens = lex("a && b || c");
        assert_eq!(tokens[0], TokenKind::Ident("a".into()));
        assert_eq!(tokens[1], TokenKind::AmpAmp);
        assert_eq!(tokens[2], TokenKind::Ident("b".into()));
        assert_eq!(tokens[3], TokenKind::PipePipe);
        assert_eq!(tokens[4], TokenKind::Ident("c".into()));
    }

    #[test]
    fn phase2_keywords() {
        let tokens = lex("lazy context pipeline stage orchestration mixin tools require");
        assert_eq!(tokens[0], TokenKind::Lazy);
        assert_eq!(tokens[1], TokenKind::Context);
        assert_eq!(tokens[2], TokenKind::Pipeline);
        assert_eq!(tokens[3], TokenKind::Stage);
        assert_eq!(tokens[4], TokenKind::Orchestration);
        assert_eq!(tokens[5], TokenKind::Mixin);
        assert_eq!(tokens[6], TokenKind::Tools);
        assert_eq!(tokens[7], TokenKind::Require);
    }
}
