use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub enum ResolvedType {
    String,
    Int,
    Float,
    Bool,
    Array(Box<ResolvedType>),
    Map(Box<ResolvedType>, Box<ResolvedType>),
    Enum(Vec<String>),
    Struct(String, Vec<(String, ResolvedType, bool)>), // name, fields (name, type, optional)
    Void,
    Unknown,
}

impl std::fmt::Display for ResolvedType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::String => write!(f, "string"),
            Self::Int => write!(f, "int"),
            Self::Float => write!(f, "float"),
            Self::Bool => write!(f, "bool"),
            Self::Array(inner) => write!(f, "{inner}[]"),
            Self::Map(k, v) => write!(f, "map<{k}, {v}>"),
            Self::Enum(variants) => {
                let vars: Vec<_> = variants.iter().map(|v| format!("\"{v}\"")).collect();
                write!(f, "enum({})", vars.join(", "))
            }
            Self::Struct(name, _) => write!(f, "{name}"),
            Self::Void => write!(f, "void"),
            Self::Unknown => write!(f, "unknown"),
        }
    }
}

pub struct TypeRegistry {
    pub types: HashMap<String, ResolvedType>,
}

impl Default for TypeRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl TypeRegistry {
    pub fn new() -> Self {
        Self {
            types: HashMap::new(),
        }
    }

    pub fn register(&mut self, name: String, ty: ResolvedType) {
        self.types.insert(name, ty);
    }

    pub fn resolve(&self, name: &str) -> Option<&ResolvedType> {
        self.types.get(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_and_resolve() {
        let mut reg = TypeRegistry::new();
        reg.register(
            "Finding".into(),
            ResolvedType::Struct(
                "Finding".into(),
                vec![("file".into(), ResolvedType::String, false)],
            ),
        );
        assert!(reg.resolve("Finding").is_some());
        assert!(reg.resolve("Nonexistent").is_none());
    }

    #[test]
    fn resolved_type_display() {
        assert_eq!(format!("{}", ResolvedType::String), "string");
        assert_eq!(
            format!("{}", ResolvedType::Array(Box::new(ResolvedType::Int))),
            "int[]"
        );
        assert_eq!(
            format!(
                "{}",
                ResolvedType::Map(Box::new(ResolvedType::String), Box::new(ResolvedType::Int))
            ),
            "map<string, int>"
        );
        assert_eq!(
            format!("{}", ResolvedType::Enum(vec!["a".into(), "b".into()])),
            "enum(\"a\", \"b\")"
        );
    }
}
