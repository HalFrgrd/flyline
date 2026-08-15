#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Tag {
    #[default]
    Blank,
    Normal,
    MultiWidthContinuation,
    Prompt,
    PromptCwdWidget(usize),
    PromptDynamicTime,
    PromptAnimation,
    PromptCopyBufferWidget,
    Command(usize),
    TabSuggestion,
    Suggestion(usize),
    HistorySuggestion,
    FuzzySearch,
    HistoryResult(usize),
    Tooltip,
    AiResult(usize),
    TabCompletionScrollBar {
        cell_height: usize,
        max_cell_height: usize,
        y_start: u16,
    },
    TutorialPrev,
    TutorialNext,
    Tutorial,
    Clipboard(ClipboardTypes),
    FlycompYes,
    FlycompNo,
    FlycompDontAsk,
    FlycompSandboxInfo,
    FlycompInfo,
    AutoCompletionTimeInfo,
    RightClickCopy,
    RightClickCut,
    RightClickPaste,
    RightClickUndo,
    RightClickRedo,
    RightClickRunTutorial,
    RightClickMenu,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ClipboardTypes {
    TutorialClickExample,
    TutorialRP1,
    TutorialMouseMode,
    TutorialAutoSuggest,
    TutorialRecommendedSettings,
    TutorialCursor0,
    TutorialCursor1,
    TutorialCursor2,
    TutorialCursor3,
    TutorialCursor4,
    TutorialCursor5,
    TutorialCursor6,
    TutorialFineGrainDeletion,
    TutorialSetColor1,
    TutorialSetColor2,
    TutorialSetColor3,
    TutorialSetColor4,
    TutorialSetColor5,
    TutorialRunHelp,
    TutorialAutoClose,
    TutorialAgentMode,
    TutorialGrep,
    TutorialBashCompletion,
    TutorialKeybindingsList,
    TutorialKeybindingsBind1,
    TutorialKeybindingsBind2,
    TutorialKeybindingsBind3,
}

pub type SpanTag = flycontent::SpanTag<Tag>;
pub type Contents = flycontent::Contents<Tag>;
pub type TaggedCell = flycontent::TaggedCell<Tag>;
pub type TaggedSpan<'a> = flycontent::TaggedSpan<'a, Tag>;
pub type TaggedLine<'a> = flycontent::TaggedLine<'a, Tag>;
