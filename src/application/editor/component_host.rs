// Copyright 2026 the Runebender Authors
// SPDX-License-Identifier: Apache-2.0

//! Editing a component in place: the session opens the component's base glyph,
//! but the canvas keeps drawing it where the composite places it.
//!
//! The text line keeps the composite as its active sort, so shaping never changes.
//! Every edit lands in the base glyph, and every glyph that uses that base follows it.
//!
//! A session also lists the composite's other editable parts.
//! With View > Components on, the canvas shows their points and a click on one moves the
//! session into that part.

use crate::application::editor::session::{PointView, Session};
use crate::application::font_model::FontModel;
use crate::application::platform::host;
use crate::application::workspace::{MetadataEdit, Mode, Tab, TextContext, Workspace};
use std::sync::Arc;

use masonry::kurbo::{Affine, BezPath, Line, Point};
use runebender::font::project::Project;
use runebender::font::variable::LayerId;
use runebender::font::{ComponentId, LayerPointType, LayerView, PointId};
use runebender::outline::glyph_paths;

/// Where the open glyph sits when it was entered from a composite.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ComponentHost {
    /// The composite at the top of the chain: the glyph the text line shows.
    pub root: String,
    /// The component entered at each level, outermost first.
    pub chain: Vec<ComponentId>,
    /// Maps the open glyph's design space into the root composite's.
    pub transform: Affine,
    /// The root composite's advance, which frames the metrics box.
    pub advance: f64,
}

/// Whose outline a [`Part`] draws, and so what a click on it opens.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PartOwner {
    /// The root composite's own contours.
    Root,
    /// One top-level component of the root composite.
    Component(ComponentId),
    /// A deeper level of the chain that can only be left, not switched to.
    Fixed,
}

/// One part of the root composite that is not the open glyph, in the root's design space.
#[derive(Clone, Debug)]
pub(crate) struct Part {
    /// What a click on this part opens.
    pub owner: PartOwner,
    /// The glyph whose layer holds this part's points.
    pub glyph: String,
    /// Everything the part draws, nested components included.
    pub outline: BezPath,
    /// The part's own points; empty for a fixed part.
    pub points: Vec<PointView>,
    /// Lines from off-curve handles to their on-curve points.
    pub handles: Vec<Line>,
}

/// Points of a layer's own contours, mapped by `transform`.
pub(crate) fn layer_points(layer: LayerView<'_>, transform: Affine) -> Vec<PointView> {
    let mut out = Vec::new();
    for contour in layer.contours() {
        for (index, point) in contour.points().enumerate() {
            let on_curve = !matches!(point.point_type(), LayerPointType::OffCurve);
            out.push(PointView {
                id: point.id(),
                point: transform * point.position(),
                on_curve,
                smooth: on_curve && point.is_smooth(),
                start: index == 0,
            });
        }
    }
    out
}

/// Handle lines of a layer's own contours, mapped by `transform`.
pub(crate) fn layer_handles(layer: LayerView<'_>, transform: Affine) -> Vec<Line> {
    let mut lines = Vec::new();
    for contour in layer.contours() {
        let points: Vec<_> = contour
            .points()
            .map(|point| (transform * point.position(), point.point_type()))
            .collect();
        let count = points.len();
        for index in 0..count {
            if !matches!(points[index].1, LayerPointType::OffCurve) {
                continue;
            }
            for neighbor in [(index + count - 1) % count, (index + 1) % count] {
                if !matches!(points[neighbor].1, LayerPointType::OffCurve) {
                    lines.push(Line::new(points[index].0, points[neighbor].0));
                }
            }
        }
    }
    lines
}

/// A chain of components, followed down from the root composite.
struct Placement {
    /// The glyph at the end of the chain: the one the session edits.
    glyph: String,
    transform: Affine,
    advance: f64,
    parts: Vec<Part>,
}

/// Follow `chain` down from `root` in one layer and collect the parts around the open glyph.
///
/// Returns `None` when a component in the chain no longer exists.
fn place(
    project: &Project,
    layer: &LayerId,
    root: &str,
    chain: &[ComponentId],
) -> Option<Placement> {
    let resolve = |glyph: &str| project.document_layer(glyph, layer);
    let root_layer = resolve(root)?;
    let advance = root_layer.width();
    let mut parts = Vec::new();
    let mut glyph = root.to_owned();
    let mut transform = Affine::IDENTITY;
    for (depth, entered) in chain
        .iter()
        .map(Some)
        .chain(std::iter::once(None))
        .enumerate()
    {
        let here = resolve(&glyph)?;
        let address = runebender::font::variable::GlyphLayerAddress {
            glyph: glyph.clone(),
            layer: layer.clone(),
        };
        let resolved =
            crate::application::editor::session::resolved_document_components(project, &address)
                .ok()?;
        let top = depth == 0;
        // The open glyph draws itself; the levels above it are parts.
        if let Some(entered) = entered {
            let own = glyph_paths::ordinary_layer_contours_to_bezpath(here);
            if top {
                if !own.elements().is_empty() {
                    parts.push(Part {
                        owner: PartOwner::Root,
                        glyph: glyph.clone(),
                        outline: own,
                        points: layer_points(here, Affine::IDENTITY),
                        handles: layer_handles(here, Affine::IDENTITY),
                    });
                }
            } else {
                parts.push(fixed(&glyph, transform * own));
            }
            for (component, view) in resolved.iter().zip(here.components()) {
                if component.id == *entered {
                    continue;
                }
                let outline = transform * component.path.clone();
                if top {
                    parts.push(component_part(project, layer, view, outline));
                } else {
                    parts.push(fixed(&glyph, outline));
                }
            }
            let view = here.components().find(|view| view.id() == *entered)?;
            transform *= view.transform();
            glyph = view.reference().to_owned();
        } else if top {
            // At the root, every component is a part the toggle can open.
            for (component, view) in resolved.iter().zip(here.components()) {
                parts.push(component_part(project, layer, view, component.path.clone()));
            }
        }
    }
    Some(Placement {
        glyph,
        transform,
        advance,
        parts,
    })
}

fn fixed(glyph: &str, outline: BezPath) -> Part {
    Part {
        owner: PartOwner::Fixed,
        glyph: glyph.to_owned(),
        outline,
        points: Vec::new(),
        handles: Vec::new(),
    }
}

fn component_part(
    project: &Project,
    layer: &LayerId,
    view: runebender::font::ComponentView<'_>,
    outline: BezPath,
) -> Part {
    let base = project.document_layer(view.reference(), layer);
    let transform = view.transform();
    Part {
        owner: PartOwner::Component(view.id()),
        glyph: view.reference().to_owned(),
        outline,
        points: base.map_or_else(Vec::new, |base| layer_points(base, transform)),
        handles: base.map_or_else(Vec::new, |base| layer_handles(base, transform)),
    }
}

impl Session {
    /// Open the glyph at the end of `chain`, placed inside `root`.
    pub(crate) fn new_in_place(
        font: &FontModel,
        root: &str,
        chain: &[ComponentId],
    ) -> Option<Self> {
        let project = &font.project;
        let source = project.source_id(project.active)?;
        let layer = project.document_source(source)?.default_layer();
        let placement = place(project, &layer, root, chain)?;
        let mut session = Self::new_from_model(font, &placement.glyph)?;
        session.set_placement(root, chain, placement);
        Some(session)
    }

    /// Reopen this session's glyph from the project, keeping its place in a composite.
    pub(crate) fn reopen(&self, font: &FontModel) -> Option<Self> {
        match &self.host {
            Some(host) => Self::new_in_place(font, &host.root, &host.chain)
                .or_else(|| Self::new_from_model(font, &host.root)),
            None => Self::new_from_model(font, &self.glyph_name),
        }
    }

    /// Recompute the host transform and the surrounding parts after the project changed.
    ///
    /// A host whose chain no longer resolves is dropped, and the session edits its glyph alone.
    pub(crate) fn refresh_placement(&mut self, project: &Project, layer: &LayerId) {
        let (root, chain) = match &self.host {
            Some(host) => (host.root.clone(), host.chain.clone()),
            None => (self.glyph_name.clone(), Vec::new()),
        };
        match place(project, layer, &root, &chain) {
            Some(placement) if placement.glyph == self.glyph_name => {
                self.set_placement(&root, &chain, placement);
            }
            _ => {
                self.host = None;
                self.parts = place(project, layer, &self.glyph_name.clone(), &[])
                    .map(|placement| Arc::new(placement.parts))
                    .unwrap_or_default();
            }
        }
    }

    fn set_placement(&mut self, root: &str, chain: &[ComponentId], placement: Placement) {
        self.host = (!chain.is_empty()).then(|| ComponentHost {
            root: root.to_owned(),
            chain: chain.to_vec(),
            transform: placement.transform,
            advance: placement.advance,
        });
        self.parts = Arc::new(placement.parts);
    }

    /// The glyph the text line shows for this session: the root composite, or the glyph.
    pub(crate) fn root_glyph(&self) -> &str {
        self.host
            .as_ref()
            .map_or(self.glyph_name.as_str(), |host| host.root.as_str())
    }

    /// Maps the open glyph's design space into the sort's.
    pub(crate) fn host_transform(&self) -> Affine {
        self.host
            .as_ref()
            .map_or(Affine::IDENTITY, |host| host.transform)
    }

    /// The advance that frames the sort: the root composite's when inside one.
    pub(crate) fn frame_advance(&self) -> f64 {
        self.host
            .as_ref()
            .map_or_else(|| self.advance(), |host| host.advance)
    }

    /// The glyph a top-level component of the open glyph refers to.
    pub(crate) fn component_reference(&self, id: ComponentId) -> Option<String> {
        self.current_layer()?
            .components()
            .find(|component| component.id() == id)
            .map(|component| component.reference().to_owned())
    }

    /// Whether the open glyph's own outline covers `at`, in its design space.
    pub(crate) fn outline_contains(&self, at: Point) -> bool {
        use masonry::kurbo::Shape as _;
        self.outline().contains(at)
    }

    /// The part whose outline is under `at`, in the root's design space.
    pub(crate) fn part_at(&self, at: Point) -> Option<PartOwner> {
        use masonry::kurbo::Shape as _;
        self.parts
            .iter()
            .rev()
            .find(|part| part.owner != PartOwner::Fixed && part.outline.contains(at))
            .map(|part| part.owner)
    }
}

impl Workspace {
    /// Open the component `id` of the open glyph in place.
    pub(crate) fn enter_component(&mut self, id: ComponentId) {
        let (root, mut chain) = self.host_chain();
        chain.push(id);
        self.open_in_place(&root, &chain, &[]);
    }

    /// Move the session to one part of the root composite, selecting `select` there.
    pub(crate) fn enter_part(&mut self, owner: PartOwner, select: &[PointId]) {
        let (root, _) = self.host_chain();
        let chain = match owner {
            PartOwner::Root => Vec::new(),
            PartOwner::Component(id) => vec![id],
            PartOwner::Fixed => return,
        };
        self.open_in_place(&root, &chain, select);
    }

    /// Leave the open component for the glyph that contains it.
    /// Returns false when the session is not inside a component.
    pub(crate) fn exit_component(&mut self) -> bool {
        let (root, mut chain) = self.host_chain();
        if chain.pop().is_none() {
            return false;
        }
        self.open_in_place(&root, &chain, &[]);
        true
    }

    /// Before undo or redo, move into the part of the composite whose glyph the next step
    /// changes, so the history replays in order across the composite and its components.
    pub(crate) fn follow_history_into_part(&mut self, redo: bool) {
        let next = if redo {
            self.metadata_redo.last()
        } else {
            self.metadata_undo.last()
        };
        let Some(MetadataEdit::DocumentLayer { glyph, .. }) = next else {
            return;
        };
        if *glyph == self.session.glyph_name {
            return;
        }
        let owner = if self
            .session
            .host
            .as_ref()
            .is_some_and(|host| host.root == *glyph)
        {
            Some(PartOwner::Root)
        } else {
            self.session
                .parts
                .iter()
                .find(|part| part.owner != PartOwner::Fixed && part.glyph == *glyph)
                .map(|part| part.owner)
        };
        if let Some(owner) = owner {
            self.enter_part(owner, &[]);
        }
    }

    /// Open `name` in a tab of its own, with its own text line, leaving this one as it is.
    pub(crate) fn open_glyph_tab(&mut self, name: &str) {
        if let Some(existing) = self
            .tabs
            .iter()
            .position(|tab| tab.session.glyph_name == name && tab.session.host.is_none())
        {
            self.activate_tab(existing);
            return;
        }
        let Some(session) = Session::new_from_model(&self.font, name) else {
            return;
        };
        let tool = self.persistent_tool();
        self.park();
        self.tabs.push(Tab {
            text_context_id: host::NEXT_TEXT_CONTEXT_ID
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            session: Arc::new(session),
            tool,
            text_context: TextContext::default(),
        });
        self.activate_tab(self.tabs.len() - 1);
    }

    fn host_chain(&self) -> (String, Vec<ComponentId>) {
        match &self.session.host {
            Some(host) => (host.root.clone(), host.chain.clone()),
            None => (self.session.glyph_name.clone(), Vec::new()),
        }
    }

    fn open_in_place(&mut self, root: &str, chain: &[ComponentId], select: &[PointId]) {
        let Some(mut session) = Session::new_in_place(&self.font, root, chain) else {
            self.note = "That component could not be opened".into();
            return;
        };
        session.viewport = self.session.viewport.clone();
        session.fitted = self.session.fitted;
        session.selection = select.iter().copied().collect();
        let name = session.glyph_name.clone();
        self.session = Arc::new(session);
        if let Some(tab) = self.tabs.get_mut(self.active_tab) {
            tab.session = self.session.clone();
        }
        if let Some(index) = self.font.index_of(&name) {
            self.selected = Some(index);
            self.mode = Mode::Editor(index);
        }
        self.selected_points = self.session.selection.len();
        self.name_buf = name;
        self.unicode_buf = self
            .session
            .codepoint()
            .map(|codepoint| format!("{:04X}", codepoint as u32))
            .unwrap_or_default();
        self.refresh_metric_bufs();
        self.refresh_coord_bufs();
        self.note.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use masonry::kurbo::{Point, Shape as _};

    fn square(name: &str, x0: f64, y0: f64, size: f64) -> norad::Glyph {
        let mut glyph = norad::Glyph::new(name);
        glyph.width = 500.0;
        let mut contour = norad::Contour::default();
        for (x, y) in [
            (x0, y0),
            (x0 + size, y0),
            (x0 + size, y0 + size),
            (x0, y0 + size),
        ] {
            contour.points.push(norad::ContourPoint::new(
                x,
                y,
                norad::PointType::Line,
                false,
                None,
                None,
            ));
        }
        glyph.contours.push(contour);
        glyph
    }

    fn component(base: &str, x: f64, y: f64) -> norad::Component {
        norad::Component::new(
            norad::Name::new(base).expect("a valid glyph name"),
            norad::AffineTransform {
                x_offset: x,
                y_offset: y,
                ..norad::AffineTransform::default()
            },
            None,
        )
    }

    /// `beh` is a dotless body and a dot; `beh.medi` uses the same two bases elsewhere.
    fn workspace(name: &str) -> Workspace {
        let path = std::env::temp_dir().join(format!(
            "runebender-component-host-{name}-{}-{}.ufo",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("the system clock is after the Unix epoch")
                .as_nanos(),
        ));
        let mut font = norad::Font::new();
        let layer = font.default_layer_mut();
        layer.insert_glyph(square("dotless", 0.0, 0.0, 400.0));
        layer.insert_glyph(square("dot", 0.0, 0.0, 100.0));
        for (name, x) in [("beh", 150.0), ("beh.medi", 50.0)] {
            let mut composite = norad::Glyph::new(name);
            composite.width = 500.0;
            composite.components.push(component("dotless", 0.0, 0.0));
            composite.components.push(component("dot", x, -200.0));
            layer.insert_glyph(composite);
        }
        font.save(&path).expect("the fixture saves");
        let mut app = Workspace::open(&path).expect("the fixture opens");
        app.open_glyph(app.font.index_of("beh").expect("beh exists"));
        app
    }

    fn component_id(app: &Workspace, index: usize) -> ComponentId {
        app.session
            .current_layer()
            .and_then(|layer| layer.components().nth(index))
            .map(|component| component.id())
            .expect("the composite has that component")
    }

    fn dot_origin(app: &Workspace, glyph: &str) -> Point {
        let address = app.font.active_layer_address(glyph).expect("glyph layer");
        let layer = app
            .font
            .project
            .document_layer(&address.glyph, &address.layer)
            .expect("layer view");
        layer
            .contours()
            .next()
            .and_then(|contour| contour.points().next())
            .map(|point| point.position())
            .expect("a point")
    }

    #[test]
    fn a_component_opens_in_place_and_closes_back_to_its_composite() {
        let mut app = workspace("in-place");
        let dot = component_id(&app, 1);
        app.enter_component(dot);
        assert_eq!(app.session.glyph_name, "dot");
        let host = app
            .session
            .host
            .as_ref()
            .expect("the dot is open inside beh");
        assert_eq!(host.root, "beh");
        assert_eq!(host.transform, Affine::translate((150.0, -200.0)));
        assert_eq!(app.session.root_glyph(), "beh");
        assert_eq!(app.session.frame_advance(), 500.0);
        // The body is the one other part, with its points placed in the composite.
        assert_eq!(app.session.parts.len(), 1);
        let body = &app.session.parts[0];
        assert_eq!(body.glyph, "dotless");
        assert_eq!(body.points.len(), 4);
        assert_eq!(
            app.session.part_at(Point::new(200.0, 200.0)),
            Some(body.owner)
        );

        assert!(app.exit_component());
        assert_eq!(app.session.glyph_name, "beh");
        assert!(app.session.host.is_none());
        assert!(
            !app.exit_component(),
            "there is nothing above the composite"
        );
    }

    #[test]
    fn an_edit_in_place_changes_the_base_and_every_composite_follows() {
        let mut app = workspace("edit");
        let dot = component_id(&app, 1);
        app.enter_component(dot);
        let mut session = (*app.session).clone();
        session.select_all();
        assert!(session.nudge_with(10.0, 20.0, false));
        app.sync_session_from(&mut session);
        app.session = Arc::new(session);

        assert_eq!(dot_origin(&app, "dot"), Point::new(10.0, 20.0));
        // Still in place after the commit reloads the session.
        assert_eq!(app.session.root_glyph(), "beh");
        // The other composite draws the edited dot where it places it.
        let medi = app
            .font
            .glyphs
            .iter()
            .find(|glyph| glyph.name == "beh.medi")
            .expect("beh.medi");
        let bounds = medi.outline.bounding_box();
        assert_eq!(bounds.min_y(), -180.0);
    }

    #[test]
    fn a_part_point_moves_the_session_and_undo_follows_the_history() {
        let mut app = workspace("part");
        let dot = component_id(&app, 1);
        let point = app.session.parts[1].points[0].id;
        app.enter_part(PartOwner::Component(dot), &[point]);
        assert_eq!(app.session.glyph_name, "dot");
        assert_eq!(app.session.selection.len(), 1);

        let mut session = (*app.session).clone();
        assert!(session.nudge_with(5.0, 0.0, false));
        app.sync_session_from(&mut session);
        app.session = Arc::new(session);
        assert_eq!(dot_origin(&app, "dot"), Point::new(5.0, 0.0));

        // Back at the body, Undo first moves into the dot, whose edit is the latest.
        let body = app.session.parts[0].owner;
        app.enter_part(body, &[]);
        assert_eq!(app.session.glyph_name, "dotless");
        app.undo_open_glyph(false);
        assert_eq!(app.session.glyph_name, "dot");
        assert_eq!(dot_origin(&app, "dot"), Point::new(0.0, 0.0));
    }

    #[test]
    fn a_base_glyph_opens_in_its_own_tab() {
        let mut app = workspace("tab");
        let tabs = app.tabs.len();
        app.open_glyph_tab("dot");
        assert_eq!(app.tabs.len(), tabs + 1);
        assert_eq!(app.session.glyph_name, "dot");
        assert!(app.session.host.is_none());
    }
}
