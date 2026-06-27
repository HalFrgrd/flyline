use crate::app::{App, ContentMode, FuzzyHistorySource};
use anyhow::Result;
use std::ops::{Add, Not};
use strum::{EnumMessage, EnumString, IntoStaticStr, VariantArray};

pub mod keyboard;
pub mod mouse;

pub use keyboard::KeyEventAction as Action;
pub use keyboard::*;

/// A single context variable that can be referenced inside a binding's
/// context expression.  Each variant evaluates to a boolean value derived
/// from the current application state.  `Always` is unconditionally `true`
/// and replaces the old `Scope::Default`.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Hash, EnumString, IntoStaticStr, EnumMessage, VariantArray,
)]
#[strum(serialize_all = "camelCase", ascii_case_insensitive)]
pub(crate) enum ContextVar {
    #[strum(message = "Always true; the catch-all context for unconditional bindings")]
    Always,
    #[strum(message = "The command buffer is empty")]
    BufferIsEmpty,
    #[strum(message = "Fuzzy history search overlay is active")]
    FuzzyHistorySearch,
    #[strum(message = "Fuzzy history search overlay for normal commands is active")]
    FuzzyHistorySearchNormalCommands,
    #[strum(message = "Fuzzy history search overlay for cancelled commands is active")]
    FuzzyHistorySearchCancelledCommands,
    #[strum(message = "Fuzzy history search overlay for agent commands is active")]
    FuzzyHistorySearchAgentCommands,
    #[strum(message = "Waiting for tab completion candidates to be produced")]
    TabCompletionWaiting,
    #[strum(message = "Tab completion overlay is active (any state)")]
    TabCompletion,
    #[strum(message = "Tab completion overlay is active and has at least one candidate")]
    TabCompletionAvailable,
    #[strum(message = "Tab completion overlay has at least one candidate and a selected entry")]
    TabCompletionEntrySelected,
    #[strum(message = "Tab completion overlay is active and has exactly one filtered candidate")]
    TabCompletionOneResult,
    #[strum(message = "Tab completion overlay is showing more than one column of candidates")]
    TabCompletionMultiColAvailable,
    #[strum(message = "Tab completion overlay is active but fuzzy filtering has no matches")]
    TabCompletionNoFilteredResults,
    #[strum(message = "Tab completion overlay is active and has no candidates at all")]
    TabCompletionNoResults,
    #[strum(message = "Tab completion was triggered by the user (not auto-started)")]
    UserTriggeredSuggestions,
    #[strum(message = "Waiting for the agent mode subprocess to finish")]
    AgentModeWaiting,
    #[strum(message = "Agent mode finished and is showing a list of selectable suggestions")]
    AgentOutputSelection,
    #[strum(message = "Agent mode failed and is showing an error message")]
    AgentModeError,
    #[strum(message = "An inline history suggestion is available to be accepted")]
    InlineSuggestionAvailable,
    #[strum(message = "Cursor is at the end of the buffer")]
    CursorAtEnd,
    #[strum(message = "Cursor is at the end of the trimmed buffer")]
    CursorAtEndTrimmed,
    #[strum(message = "Cursor is at the start of the buffer")]
    CursorAtStart,
    #[strum(message = "Cursor is on the first line of the buffer")]
    CursorOnFirstLine,
    #[strum(message = "Cursor is on the final line of the buffer")]
    CursorOnFinalLine,
    #[strum(message = "Prompt directory selection mode is active")]
    PromptDirSelection,
    #[strum(message = "There is an active text selection in the buffer")]
    TextSelected,
    #[strum(message = "The command buffer contains at least one newline")]
    MultilineBuffer,
    #[strum(message = "The command buffer starts with an agent mode prefix")]
    BufferHasAgentModePrefix,
    #[strum(message = "The content mode is normal editing (no overlay is active)")]
    EditingBufferMode,
    #[strum(message = "Prompting the user whether they want to run flycomp")]
    TabCompletionAskForFlycomp,
    #[strum(message = "Flycomp completion synthesis is currently running in the background")]
    TabCompletionRunningFlycomp,
    #[strum(message = "Flycomp completion synthesis finished and has a result or error")]
    TabCompletionFlycompResult,
    #[strum(message = "Fuzzy history search overlay is active and no entry is currently selected")]
    FuzzyHistorySearchNoneSelected,
    #[strum(message = "Agent output selection is active and no suggestion is currently selected")]
    AgentOutputNoneSelected,
}

impl ContextVar {
    pub(crate) fn as_str(&self) -> &'static str {
        <&'static str>::from(*self)
    }

    pub(crate) fn evaluate(&self, app: &App) -> bool {
        match self {
            ContextVar::Always => true,
            ContextVar::BufferIsEmpty => app.buffer.buffer().is_empty(),
            ContextVar::FuzzyHistorySearch => {
                matches!(app.content_mode, ContentMode::FuzzyHistorySearch(_))
            }
            ContextVar::FuzzyHistorySearchNormalCommands => {
                matches!(
                    app.content_mode,
                    ContentMode::FuzzyHistorySearch(FuzzyHistorySource::PastCommands)
                )
            }
            ContextVar::FuzzyHistorySearchCancelledCommands => {
                matches!(
                    app.content_mode,
                    ContentMode::FuzzyHistorySearch(FuzzyHistorySource::CancelledCommands)
                )
            }
            ContextVar::FuzzyHistorySearchAgentCommands => {
                matches!(
                    app.content_mode,
                    ContentMode::FuzzyHistorySearch(FuzzyHistorySource::AgentPrompts)
                )
            }
            ContextVar::TabCompletionWaiting => {
                matches!(app.content_mode, ContentMode::TabCompletionWaiting { .. })
            }
            ContextVar::TabCompletion => {
                matches!(app.content_mode, ContentMode::TabCompletion { .. })
            }
            ContextVar::TabCompletionAvailable => matches!(
                &app.content_mode,
                ContentMode::TabCompletion(active_suggestions)
                    if active_suggestions.filtered_suggestions_len() > 0
            ),
            ContextVar::TabCompletionEntrySelected => matches!(
                &app.content_mode,
                ContentMode::TabCompletion(active_suggestions)
                    if active_suggestions.filtered_suggestions_len() > 0
                        && active_suggestions.selected_coord.is_some()
            ),
            ContextVar::TabCompletionOneResult => matches!(
                &app.content_mode,
                ContentMode::TabCompletion(active_suggestions)
                    if active_suggestions.filtered_suggestions_len() == 1
            ),
            ContextVar::TabCompletionMultiColAvailable => matches!(
                &app.content_mode,
                ContentMode::TabCompletion(active_suggestions)
                    if active_suggestions.last_num_data_cols > 1
            ),
            ContextVar::TabCompletionNoFilteredResults => matches!(
                &app.content_mode,
                ContentMode::TabCompletion(active_suggestions)
                    if active_suggestions.filtered_suggestions_len() == 0
            ),
            ContextVar::TabCompletionNoResults => matches!(
                &app.content_mode,
                ContentMode::TabCompletion(active_suggestions)
                    if active_suggestions.all_suggestions_len() == 0
            ),
            ContextVar::UserTriggeredSuggestions => matches!(
                &app.content_mode,
                ContentMode::TabCompletion(active_suggestions)
                    if !active_suggestions.auto_started
            ),
            ContextVar::AgentModeWaiting => {
                matches!(app.content_mode, ContentMode::AgentModeWaiting { .. })
            }
            ContextVar::AgentOutputSelection => {
                matches!(app.content_mode, ContentMode::AgentOutputSelection { .. })
            }
            ContextVar::AgentModeError => {
                matches!(app.content_mode, ContentMode::AgentError { .. })
            }
            ContextVar::InlineSuggestionAvailable => app.inline_history_suggestion.is_some(),
            ContextVar::CursorAtEnd => app.buffer.is_cursor_at_end(),
            ContextVar::CursorAtEndTrimmed => app.buffer.is_cursor_at_trimmed_end(),
            ContextVar::CursorAtStart => app.buffer.is_cursor_at_start(),
            ContextVar::CursorOnFirstLine => app.buffer.cursor_row() == 0,
            ContextVar::CursorOnFinalLine => app.buffer.is_cursor_on_final_line(),
            ContextVar::PromptDirSelection => {
                matches!(app.content_mode, ContentMode::PromptDirSelect(_))
            }
            ContextVar::TextSelected => app.buffer.selection_range().is_some(),
            ContextVar::MultilineBuffer => app.buffer.buffer().contains('\n'),
            ContextVar::BufferHasAgentModePrefix => {
                app.buffer_starts_with_agent_command_prefix().is_some()
            }
            ContextVar::EditingBufferMode => matches!(app.content_mode, ContentMode::Normal),
            ContextVar::TabCompletionAskForFlycomp => {
                matches!(
                    app.content_mode,
                    ContentMode::TabCompletionAskForFlycomp { .. }
                )
            }
            ContextVar::TabCompletionRunningFlycomp => {
                matches!(
                    app.content_mode,
                    ContentMode::TabCompletionRunningFlycomp { .. }
                )
            }
            ContextVar::TabCompletionFlycompResult => {
                matches!(
                    app.content_mode,
                    ContentMode::TabCompletionFlycompResult { .. }
                )
            }
            ContextVar::FuzzyHistorySearchNoneSelected => {
                if let ContentMode::FuzzyHistorySearch(ref source) = app.content_mode {
                    app.select_fuzzy_history_manager(source)
                        .fuzzy_search_idx()
                        .is_none()
                } else {
                    false
                }
            }
            ContextVar::AgentOutputNoneSelected => {
                if let ContentMode::AgentOutputSelection(ref selection) = app.content_mode {
                    selection.selected_idx.is_none()
                } else {
                    false
                }
            }
        }
    }
}

/// Cached snapshot of all context variables for a single input event.
///
/// Computed once per event in event handlers and reused by every
/// binding's context expression evaluation.
pub(crate) struct ContextValues {
    values: [bool; <ContextVar as VariantArray>::VARIANTS.len()],
}

impl ContextValues {
    pub fn evaluate(app: &App) -> Self {
        let mut values = [false; <ContextVar as VariantArray>::VARIANTS.len()];
        for (i, v) in <ContextVar as VariantArray>::VARIANTS.iter().enumerate() {
            values[i] = v.evaluate(app);
        }
        Self { values }
    }

    fn index_of(var: ContextVar) -> usize {
        <ContextVar as VariantArray>::VARIANTS
            .iter()
            .position(|v| *v == var)
            .expect("ContextVar must be in ContextVar::VARIANTS")
    }

    pub fn get(&self, var: ContextVar) -> bool {
        self.values[Self::index_of(var)]
    }
}

/// A single literal in a context expression: a variable, optionally negated.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ContextLiteral {
    pub(crate) var: ContextVar,
    pub(crate) negated: bool,
}

impl ContextLiteral {
    pub(crate) fn new(var: ContextVar, negated: bool) -> Self {
        Self { var, negated }
    }

    fn negate(&self) -> Self {
        Self {
            var: self.var,
            negated: !self.negated,
        }
    }
}

impl Into<ContextLiteral> for ContextVar {
    fn into(self) -> ContextLiteral {
        ContextLiteral {
            var: self,
            negated: false,
        }
    }
}

impl From<ContextVar> for ContextExpr {
    fn from(value: ContextVar) -> Self {
        Self::new(vec![value.into()])
    }
}

impl From<ContextLiteral> for ContextExpr {
    fn from(value: ContextLiteral) -> Self {
        Self::new(vec![value])
    }
}

impl Not for ContextVar {
    type Output = ContextLiteral;

    fn not(self) -> Self::Output {
        ContextLiteral::new(self, true)
    }
}

impl Not for ContextLiteral {
    type Output = ContextLiteral;

    fn not(self) -> Self::Output {
        self.negate()
    }
}

/// A context expression: a conjunction (AND-chain) of literals.
///
/// The grammar is intentionally small: a `+`-separated list of context
/// variable names, each optionally prefixed with `!` for negation.
/// Parentheses and `||` are not supported.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ContextExpr {
    pub(crate) literals: Vec<ContextLiteral>,
}

impl ContextExpr {
    pub fn new(literals: Vec<ContextLiteral>) -> Self {
        Self { literals }
    }

    /// Evaluate the expression against the precomputed context values.
    pub fn evaluate(&self, ctx: &ContextValues) -> bool {
        self.literals.iter().all(|lit| {
            let v = ctx.get(lit.var);
            if lit.negated { !v } else { v }
        })
    }

    /// Render the expression in canonical form (e.g. `a+!b+c`).
    pub fn display(&self) -> String {
        if self.literals.is_empty() {
            return ContextVar::Always.as_str().to_string();
        }
        self.literals
            .iter()
            .map(|lit| {
                if lit.negated {
                    format!("!{}", lit.var.as_str())
                } else {
                    lit.var.as_str().to_string()
                }
            })
            .collect::<Vec<_>>()
            .join("+")
    }
}

impl<Rhs> Add<Rhs> for ContextVar
where
    Rhs: Into<ContextExpr>,
{
    type Output = ContextExpr;

    fn add(self, rhs: Rhs) -> Self::Output {
        ContextExpr::from(self) + rhs
    }
}

impl<Rhs> Add<Rhs> for ContextLiteral
where
    Rhs: Into<ContextExpr>,
{
    type Output = ContextExpr;

    fn add(self, rhs: Rhs) -> Self::Output {
        ContextExpr::from(self) + rhs
    }
}

impl<Rhs> Add<Rhs> for ContextExpr
where
    Rhs: Into<ContextExpr>,
{
    type Output = ContextExpr;

    fn add(mut self, rhs: Rhs) -> Self::Output {
        self.literals.extend(rhs.into().literals);
        self
    }
}

impl TryFrom<&str> for ContextExpr {
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
            let var = ContextVar::try_from(name)?;
            literals.push(ContextLiteral { var, negated });
        }
        Ok(Self { literals })
    }
}
