// runebender-core — platform-independent types shared between
// runebender-xilem (native, kurbo 0.12 via masonry 0.4) and
// runebender-comfy (WASM, kurbo 0.13 via vello+peniko).
//
// Only the modules with NO kurbo geometry types in their public API
// live here. Anything that takes/returns `kurbo::Point`,
// `kurbo::Affine`, `kurbo::BezPath`, etc. stays duplicated in each
// project until the two consumers can be aligned on one kurbo
// version. See the project SECURITY.md and runebender-comfy README
// for the broader plan.

pub mod editing;
pub mod model;
