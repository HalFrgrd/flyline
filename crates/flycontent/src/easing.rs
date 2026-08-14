use easing_function::Easing as _;
use easing_function::easings::StandardEasing;
use strum::{AsRefStr, EnumString, VariantArray};

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Default,
    AsRefStr,
    VariantArray,
    EnumString,
    serde::Serialize,
    serde::Deserialize,
)]
#[strum(serialize_all = "kebab-case")]
pub enum CursorEasing {
    #[default]
    Linear,
    InQuad,
    OutQuad,
    InOutQuad,
    InCubic,
    OutCubic,
    InOutCubic,
    InQuart,
    OutQuart,
    InOutQuart,
    InQuint,
    OutQuint,
    InOutQuint,
    InSine,
    OutSine,
    InOutSine,
    InCirc,
    OutCirc,
    InOutCirc,
    InExpo,
    OutExpo,
    InOutExpo,
    InElastic,
    OutElastic,
    InOutElastic,
    InBack,
    OutBack,
    InOutBack,
    InBounce,
    OutBounce,
    InOutBounce,
}

impl CursorEasing {
    /// Apply the easing function to `t` ∈ [0, 1], returning a value in [0, 1].
    pub fn apply(self, t: f32) -> f32 {
        match self {
            CursorEasing::Linear => StandardEasing::Linear.ease(t),
            CursorEasing::InQuad => StandardEasing::InQuadradic.ease(t),
            CursorEasing::OutQuad => StandardEasing::OutQuadradic.ease(t),
            CursorEasing::InOutQuad => StandardEasing::InOutQuadradic.ease(t),
            CursorEasing::InCubic => StandardEasing::InCubic.ease(t),
            CursorEasing::OutCubic => StandardEasing::OutCubic.ease(t),
            CursorEasing::InOutCubic => StandardEasing::InOutCubic.ease(t),
            CursorEasing::InQuart => StandardEasing::InQuartic.ease(t),
            CursorEasing::OutQuart => StandardEasing::OutQuartic.ease(t),
            CursorEasing::InOutQuart => StandardEasing::InOutQuartic.ease(t),
            CursorEasing::InQuint => StandardEasing::InQuintic.ease(t),
            CursorEasing::OutQuint => StandardEasing::OutQuintic.ease(t),
            CursorEasing::InOutQuint => StandardEasing::InOutQuintic.ease(t),
            CursorEasing::InSine => StandardEasing::InSine.ease(t),
            CursorEasing::OutSine => StandardEasing::OutSine.ease(t),
            CursorEasing::InOutSine => StandardEasing::InOutSine.ease(t),
            CursorEasing::InCirc => StandardEasing::InCircular.ease(t),
            CursorEasing::OutCirc => StandardEasing::OutCircular.ease(t),
            CursorEasing::InOutCirc => StandardEasing::InOutCircular.ease(t),
            CursorEasing::InExpo => StandardEasing::InExponential.ease(t),
            CursorEasing::OutExpo => StandardEasing::OutExponential.ease(t),
            CursorEasing::InOutExpo => StandardEasing::InOutExponential.ease(t),
            CursorEasing::InElastic => StandardEasing::InElastic.ease(t),
            CursorEasing::OutElastic => StandardEasing::OutElastic.ease(t),
            CursorEasing::InOutElastic => StandardEasing::InOutElastic.ease(t),
            CursorEasing::InBack => StandardEasing::InBack.ease(t),
            CursorEasing::OutBack => StandardEasing::OutBack.ease(t),
            CursorEasing::InOutBack => StandardEasing::InOutBack.ease(t),
            CursorEasing::InBounce => StandardEasing::InBounce.ease(t),
            CursorEasing::OutBounce => StandardEasing::OutBounce.ease(t),
            CursorEasing::InOutBounce => StandardEasing::InOutBounce.ease(t),
        }
    }
}

/// Compute the cursor intensity for the fade effect from a raw oscillation value
/// `raw_t` ∈ [0, 1] (e.g. the output of a sine wave) and the given easing function.
///
/// Maps the eased value to [0.2, 1.0] so the cursor is always at least faintly visible.
pub fn fade_intensity(raw_t: f32, easing: CursorEasing) -> f32 {
    let eased = easing.apply(raw_t.clamp(0.0, 1.0));
    // Map eased [0, 1] → [0.2, 1.0] so the cursor never fully disappears.
    eased * 0.8 + 0.2
}
