#[derive(Debug, Clone, PartialEq)]
pub enum Hint {
    Input(String),
    Alloc(usize),
    RandomEcPoint,
}
