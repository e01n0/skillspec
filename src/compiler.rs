use crate::ast::{Skill, SourceFile};

pub trait TargetCompiler {
    fn name(&self) -> &str;
    fn file_extension(&self) -> &str;
    fn compile_skill(&self, skill: &Skill, source: &SourceFile) -> String;
}
