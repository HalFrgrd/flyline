use crate::app::App;
use anyhow::Result;
use std::ops::{Add, Not};

pub mod keyboard;
pub mod mouse;

pub use keyboard::*;

pub trait ContextVar: std::fmt::Debug + Clone + Copy + PartialEq + Eq {
    fn evaluate(&self, app: &App) -> bool;
    fn display(&self) -> String;
}

/// Cached snapshot of context variables for a single input event.
pub struct ContextValues<V: strum::VariantArray> {
    values: Vec<bool>,
    _marker: std::marker::PhantomData<V>,
}

impl<V> ContextValues<V>
where
    V: ContextVar + strum::VariantArray + 'static,
{
    pub fn evaluate(app: &App) -> Self {
        let mut values = Vec::new();
        for v in V::VARIANTS.iter() {
            values.push(v.evaluate(app));
        }
        Self {
            values,
            _marker: std::marker::PhantomData,
        }
    }

    fn index_of(var: V) -> usize {
        V::VARIANTS
            .iter()
            .position(|v| *v == var)
            .expect("Variable must be in VARIANTS")
    }

    pub fn get(&self, var: V) -> bool {
        self.values[Self::index_of(var)]
    }
}

/// A single literal in a context expression: a variable, optionally negated.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ContextLiteral<V> {
    pub var: V,
    pub negated: bool,
}

impl<V: ContextVar> ContextLiteral<V> {
    pub fn new(var: V, negated: bool) -> Self {
        Self { var, negated }
    }

    fn negate(&self) -> Self {
        Self {
            var: self.var,
            negated: !self.negated,
        }
    }
}

impl<V: ContextVar> From<V> for ContextExpr<V> {
    fn from(value: V) -> Self {
        Self::new(vec![ContextLiteral {
            var: value,
            negated: false,
        }])
    }
}

impl<V: ContextVar> From<ContextLiteral<V>> for ContextExpr<V> {
    fn from(value: ContextLiteral<V>) -> Self {
        Self::new(vec![value])
    }
}

impl<V: ContextVar> Not for ContextLiteral<V> {
    type Output = ContextLiteral<V>;

    fn not(self) -> Self::Output {
        self.negate()
    }
}

/// A context expression: a conjunction (AND-chain) of literals.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContextExpr<V> {
    pub literals: Vec<ContextLiteral<V>>,
}

impl<V: ContextVar> ContextExpr<V> {
    pub fn new(literals: Vec<ContextLiteral<V>>) -> Self {
        Self { literals }
    }

    pub fn evaluate_direct(&self, app: &App) -> bool {
        self.literals.iter().all(|lit| {
            let v = lit.var.evaluate(app);
            if lit.negated { !v } else { v }
        })
    }

    pub fn display(&self) -> String {
        self.literals
            .iter()
            .map(|lit| {
                if lit.negated {
                    format!("!{}", lit.var.display())
                } else {
                    lit.var.display()
                }
            })
            .collect::<Vec<_>>()
            .join("+")
    }
}

impl<V> ContextExpr<V>
where
    V: ContextVar + strum::VariantArray + 'static,
{
    /// Evaluate the expression against the precomputed context values.
    pub fn evaluate(&self, ctx: &ContextValues<V>) -> bool {
        self.literals.iter().all(|lit| {
            let v = ctx.get(lit.var);
            if lit.negated { !v } else { v }
        })
    }
}

impl<Rhs, V> Add<Rhs> for ContextLiteral<V>
where
    V: ContextVar,
    Rhs: Into<ContextExpr<V>>,
{
    type Output = ContextExpr<V>;

    fn add(self, rhs: Rhs) -> Self::Output {
        ContextExpr::from(self) + rhs
    }
}

impl<Rhs, V> Add<Rhs> for ContextExpr<V>
where
    V: ContextVar,
    Rhs: Into<ContextExpr<V>>,
{
    type Output = ContextExpr<V>;

    fn add(mut self, rhs: Rhs) -> Self::Output {
        self.literals.extend(rhs.into().literals);
        self
    }
}

impl<V> TryFrom<&str> for ContextExpr<V>
where
    V: ContextVar + std::str::FromStr,
    <V as std::str::FromStr>::Err: std::fmt::Debug,
{
    type Error = anyhow::Error;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        let s = s.trim();
        if s.is_empty() {
            return Err(anyhow::anyhow!("Empty context expression"));
        }
        if s.contains("&&") || s.contains("||") {
            return Err(anyhow::anyhow!(
                "Context expressions only support '+' as a separator (no '&&' or '||'): '{}'",
                s
            ));
        }
        if s.contains('(') || s.contains(')') {
            return Err(anyhow::anyhow!(
                "Context expressions do not support parentheses: '{}'",
                s
            ));
        }
        let mut literals = Vec::new();
        for raw in s.split('+') {
            let raw = raw.trim();
            if raw.is_empty() {
                return Err(anyhow::anyhow!(
                    "Empty literal in context expression: '{}'",
                    s
                ));
            }
            let (negated, name) = if let Some(rest) = raw.strip_prefix('!') {
                (true, rest.trim())
            } else {
                (false, raw)
            };
            if name.is_empty() {
                return Err(anyhow::anyhow!(
                    "Missing variable name after '!' in context expression: '{}'",
                    s
                ));
            }
            let var = V::from_str(name)
                .map_err(|e| anyhow::anyhow!("Unknown context variable '{}': {:?}", name, e))?;
            literals.push(ContextLiteral { var, negated });
        }
        Ok(Self { literals })
    }
}
