#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Language {
    #[default]
    Auto,
    JavaScript,
    TypeScript,
    Jsx,
    Tsx,
}
