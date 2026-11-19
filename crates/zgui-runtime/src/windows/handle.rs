//! One window, as the application holds it.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::{Arc, Weak};

use zgui_geom::{Css, CssPx, Point, Size};
use zgui_platform::{
    ColorScheme, CursorStyle, DecorationSource, Decorations, FullscreenMode, PlatformCapabilities,
    ResizeEdge, Surface, WindowIcon, WindowLevel,
};
use zgui_reactive::prelude::*;
use zgui_reactive::{ArcRwSignal, Signal};

use crate::commands::{WindowCommands, WindowToken};
use crate::windows::Zoom;

/// Which window this is.
///
/// Stable for the window's whole life, including across a suspend that takes its surface away and
/// gives it a different one back.
pub type WindowId = WindowToken;

/// One window of the application.
///
/// Cheap to clone and safe to keep: **every operation on a window that has closed is a silent
/// no-op**, and so is every operation a desktop cannot carry out. That is the whole cross-platform
/// contract — an application asks for what it wants once, and runs everywhere without a branch per
/// platform. What a desktop can actually do is readable through [`WindowHandle::capabilities`] for
/// an application that wants to show or hide an affordance rather than let it do nothing.
///
/// Bound to the thread the windows run on, like every other view-layer handle. Work on another
/// thread reaches a window the way it reaches anything else: by posting to the UI thread.
#[derive(Clone)]
pub struct WindowHandle {
    /// What every clone of this handle shares.
    shared: Rc<WindowShared>,
}

/// The state behind every clone of one window's handle.
///
/// The signals are reference-counted and have no reactive owner. Thus they stay alive as long as
/// the handle. The code that opens a window can unmount while the window stays open.
pub(crate) struct WindowShared {
    /// Which window this is.
    pub(crate) id: WindowId,
    /// The surface it draws into, while it has one.
    ///
    /// Weak, because a handle must not keep a window alive: an application holding the handle of a
    /// window the user closed is holding a name, not the window. Absent before it first opens and
    /// while a suspended platform is holding every surface.
    pub(crate) surface: RefCell<Option<Weak<dyn Surface>>>,
    /// Whether the window has closed for good.
    pub(crate) closed: Cell<bool>,
    /// Whether a platform-driven move or resize was begun since the last frame.
    ///
    /// Read by the frame, which then tells the router the press it was tracking is over: the
    /// compositor has taken the pointer and no release will ever arrive.
    pub(crate) drag_started: Cell<bool>,
    /// Where a close or an open is asked for.
    pub(crate) commands: WindowCommands,
    /// What this desktop can do, once the platform has said.
    pub(crate) capabilities: Rc<RefCell<PlatformCapabilities>>,
    /// The content size, in CSS pixels at the current zoom.
    pub(crate) size: ArcRwSignal<Size<CssPx, Css>>,
    /// How many physical pixels there are to a CSS pixel on the display, without the zoom.
    pub(crate) scale: ArcRwSignal<f32>,
    /// The zoom the window draws at, written together with `size`.
    pub(crate) zoom: ArcRwSignal<Zoom>,
    /// The zoom the application asked for, which the runtime applies on the next turn of the loop
    /// or when the window opens.
    pub(crate) requested_zoom: Cell<Zoom>,
    /// Whether the window holds the keyboard.
    pub(crate) focused: ArcRwSignal<bool>,
    /// Whether the desktop says nothing of the window is visible.
    pub(crate) occluded: ArcRwSignal<bool>,
    /// Whether the window is maximised.
    pub(crate) maximized: ArcRwSignal<bool>,
    /// How the window fills the screen, if it does.
    pub(crate) fullscreen: ArcRwSignal<Option<FullscreenMode>>,
    /// The light or dark scheme the window's document is styled in.
    pub(crate) color_scheme: ArcRwSignal<ColorScheme>,
    /// Who draws the window frame.
    pub(crate) decoration_source: ArcRwSignal<DecorationSource>,
}

impl WindowHandle {
    /// A handle for a window that has been asked for and does not exist yet.
    pub(crate) fn pending(
        id: WindowId,
        commands: WindowCommands,
        capabilities: Rc<RefCell<PlatformCapabilities>>,
        zoom: Zoom,
    ) -> Self {
        // Replaced by the answer of the surface when the window opens.
        let decoration_source = ArcRwSignal::new(capabilities.borrow().decorations);
        Self {
            shared: Rc::new(WindowShared {
                id,
                surface: RefCell::new(None),
                closed: Cell::new(false),
                drag_started: Cell::new(false),
                commands,
                capabilities,
                size: ArcRwSignal::new(Size::new(CssPx(0.0), CssPx(0.0))),
                scale: ArcRwSignal::new(1.0),
                zoom: ArcRwSignal::new(zoom),
                requested_zoom: Cell::new(zoom),
                // A window is opened focused until the desktop says otherwise, which is the same
                // assumption the window itself starts from.
                focused: ArcRwSignal::new(true),
                occluded: ArcRwSignal::new(false),
                maximized: ArcRwSignal::new(false),
                fullscreen: ArcRwSignal::new(None),
                // The scheme a window's document starts in until the desktop states one.
                color_scheme: ArcRwSignal::new(ColorScheme::Light),
                decoration_source,
            }),
        }
    }

    /// Gives the handle the surface its window has just been opened on.
    ///
    /// What it reports is seeded from the surface rather than from what was asked for: a desktop is
    /// free to open a window at a different size, maximised, or on a display of another density.
    pub(crate) fn attach(&self, surface: &Arc<dyn Surface>) {
        *self.shared.surface.borrow_mut() = Some(Arc::downgrade(surface));
        let scale = surface.scale_factor() as f32;
        let size = surface.size();
        let zoom = self.shared.requested_zoom.get();
        let css = scale * zoom.get();
        self.shared.scale.set(scale);
        self.shared.zoom.set(zoom);
        self.shared.size.set(Size::new(
            CssPx(size.width.0 / css),
            CssPx(size.height.0 / css),
        ));
        self.shared.maximized.set(surface.is_maximized());
        self.shared.fullscreen.set(surface.fullscreen());
        self.shared
            .decoration_source
            .set(surface.decoration_source());
    }

    /// Takes the surface away, leaving the window named but not open.
    ///
    /// What a suspend does. The signals keep their last values: an application reading the size of
    /// a suspended window is better served by the size it had than by zero.
    pub(crate) fn detach(&self) {
        *self.shared.surface.borrow_mut() = None;
    }

    /// Records that the window has closed for good.
    pub(crate) fn note_closed(&self) {
        self.shared.closed.set(true);
        self.detach();
    }

    /// Whether a platform-driven drag was begun since this was last asked, clearing the record.
    pub(crate) fn take_drag_started(&self) -> bool {
        self.shared.drag_started.replace(false)
    }

    /// Records the size and density the window actually turned out to have.
    ///
    /// Each signal is written only when its value moved. A drag delivers a configure per step, and
    /// a step that changed the width alone must wake the readers of the width alone — a scale
    /// signal that fires with an unchanged value re-runs every effect reading it, once per step.
    pub(crate) fn set_geometry(
        &self,
        size: zgui_geom::Size<zgui_geom::DevicePx, zgui_geom::Device>,
        platform_scale: f32,
        zoom: Zoom,
    ) {
        if self.shared.scale.get_untracked() != platform_scale {
            self.shared.scale.set(platform_scale);
        }
        if self.shared.zoom.get_untracked() != zoom {
            self.shared.zoom.set(zoom);
        }
        let css = platform_scale * zoom.get();
        let size = Size::new(CssPx(size.width.0 / css), CssPx(size.height.0 / css));
        if self.shared.size.get_untracked() != size {
            self.shared.size.set(size);
        }
    }

    /// The zoom the application asked for, which the window opens at or changes to.
    pub(crate) fn requested_zoom(&self) -> Zoom {
        self.shared.requested_zoom.get()
    }

    /// Records a zoom the window was given directly, so a reopened window opens at it.
    pub(crate) fn note_zoom(&self, zoom: Zoom) {
        self.shared.requested_zoom.set(zoom);
    }

    /// Records whether the window holds the keyboard.
    pub(crate) fn set_focused(&self, focused: bool) {
        self.shared.focused.set(focused);
    }

    /// Records whether the desktop says nothing of the window is visible.
    pub(crate) fn set_occluded(&self, occluded: bool) {
        self.shared.occluded.set(occluded);
    }

    /// Records the scheme the window's document is styled in.
    ///
    /// The signal is written only when the value changed, because a desktop can state the same
    /// preference again.
    pub(crate) fn set_color_scheme(&self, scheme: ColorScheme) {
        if self.shared.color_scheme.get_untracked() != scheme {
            self.shared.color_scheme.set(scheme);
        }
    }

    /// Records who the platform says draws the window frame.
    ///
    /// Reports whether the value changed.
    pub(crate) fn set_decoration_source(&self, source: DecorationSource) -> bool {
        let moved = self.shared.decoration_source.get_untracked() != source;
        if moved {
            self.shared.decoration_source.set(source);
        }
        moved
    }

    /// Asks the surface again whether it is maximised or full screen.
    ///
    /// Neither is reported as an event by any desktop, and both always resize the window — so a
    /// resize is where they are noticed.
    pub(crate) fn refresh_window_state(&self) {
        if let Some(surface) = self.surface() {
            let maximized = surface.is_maximized();
            if self.shared.maximized.get_untracked() != maximized {
                self.shared.maximized.set(maximized);
            }
            let fullscreen = surface.fullscreen();
            if self.shared.fullscreen.get_untracked() != fullscreen {
                self.shared.fullscreen.set(fullscreen);
            }
        }
    }

    /// Which window this is.
    pub fn id(&self) -> WindowId {
        self.shared.id
    }

    /// What this desktop can do.
    ///
    /// For an application that would rather hide an affordance than offer one that does nothing —
    /// a "move window here" menu item on a desktop that does not let a window place itself. Every
    /// operation is safe to call whatever this says.
    pub fn capabilities(&self) -> PlatformCapabilities {
        self.shared.capabilities.borrow().clone()
    }

    /// Whether the window is open.
    pub fn is_open(&self) -> bool {
        self.surface().is_some()
    }

    /// The content size, in CSS pixels at the current zoom.
    ///
    /// This is the viewport the document is laid out in: the surface size in device pixels divided
    /// by [`WindowHandle::scale`] and by [`WindowHandle::zoom`].
    pub fn size(&self) -> Signal<Size<CssPx, Css>> {
        self.shared.size.clone().into()
    }

    /// How many physical pixels there are to a CSS pixel on this window's display.
    ///
    /// This is the platform scale factor. It does not include the zoom.
    pub fn scale(&self) -> Signal<f32> {
        self.shared.scale.clone().into()
    }

    /// How much the window scales its content.
    ///
    /// This changes together with [`WindowHandle::size`], when the window applies a zoom that
    /// [`WindowHandle::set_zoom`] asked for.
    pub fn zoom(&self) -> Signal<Zoom> {
        self.shared.zoom.clone().into()
    }

    /// Scales the window's content by `zoom`, as a browser's zoom control does.
    ///
    /// The zoom multiplies the platform scale factor. The document is laid out again in the CSS
    /// viewport at the new zoom, text is rasterised again at the new size, and pointer positions
    /// are mapped through the zoom. The window's own size on the desktop does not change.
    ///
    /// The window applies the zoom on the next turn of the loop. A window that has not opened yet
    /// opens at `zoom`.
    pub fn set_zoom(&self, zoom: Zoom) {
        if self.shared.closed.get() || self.shared.requested_zoom.get() == zoom {
            return;
        }
        self.shared.requested_zoom.set(zoom);
        self.shared.commands.zoom(self.shared.id);
    }

    /// Whether the window holds the keyboard.
    pub fn focused(&self) -> Signal<bool> {
        self.shared.focused.clone().into()
    }

    /// Whether the desktop says nothing of the window is visible.
    ///
    /// What an application watches to stop animating something nobody can see.
    pub fn occluded(&self) -> Signal<bool> {
        self.shared.occluded.clone().into()
    }

    /// Whether the window is maximised.
    pub fn maximized(&self) -> Signal<bool> {
        self.shared.maximized.clone().into()
    }

    /// How the window fills the screen, if it does.
    pub fn fullscreen(&self) -> Signal<Option<FullscreenMode>> {
        self.shared.fullscreen.clone().into()
    }

    /// The light or dark scheme the window's document is styled in.
    ///
    /// The `prefers-color-scheme` media query matches against this value. It is light until the
    /// platform reports a preference for the window.
    pub fn color_scheme(&self) -> Signal<ColorScheme> {
        self.shared.color_scheme.clone().into()
    }

    /// Who draws the title bar and the frame of this window.
    ///
    /// The value comes from the platform, for this window, and it can change while the window is
    /// open. A window that asks for [`Decorations::Full`] and sees
    /// [`DecorationSource::Application`] gets no frame from the platform. It must then draw its own
    /// title bar, and give the user a way to move, resize and close the window. A Wayland
    /// compositor without server-side decorations gives this answer.
    pub fn decoration_source(&self) -> Signal<DecorationSource> {
        self.shared.decoration_source.clone().into()
    }

    /// The surface, while there is one to act on.
    fn surface(&self) -> Option<Arc<dyn Surface>> {
        if self.shared.closed.get() {
            return None;
        }
        self.shared
            .surface
            .borrow()
            .as_ref()
            .and_then(Weak::upgrade)
    }

    /// Runs `act` on the surface, or does nothing at all.
    fn act(&self, act: impl FnOnce(&Arc<dyn Surface>)) {
        if let Some(surface) = self.surface() {
            act(&surface);
        }
    }

    /// Names the window.
    pub fn set_title(&self, title: impl AsRef<str>) {
        self.act(|surface| surface.set_title(title.as_ref()));
    }

    /// Asks for a different content size, in CSS pixels.
    ///
    /// A request: a desktop may answer with a different size, or refuse. Watch
    /// [`WindowHandle::size`] for what was actually taken. The size is in the desktop's CSS
    /// pixels, which do not include the zoom.
    pub fn request_size(&self, width: f32, height: f32) {
        self.act(|surface| {
            surface.request_size(Size::new(CssPx(width), CssPx(height)));
        });
    }

    /// Sets the smallest size the user may drag to, in CSS pixels.
    pub fn set_min_size(&self, size: Option<(f32, f32)>) {
        self.act(|surface| {
            surface.set_min_size(size.map(|(w, h)| Size::new(CssPx(w), CssPx(h))));
        });
    }

    /// Sets the largest size the user may drag to, in CSS pixels.
    pub fn set_max_size(&self, size: Option<(f32, f32)>) {
        self.act(|surface| {
            surface.set_max_size(size.map(|(w, h)| Size::new(CssPx(w), CssPx(h))));
        });
    }

    /// Asks for the window to be placed at a point on the desktop.
    ///
    /// Does nothing where a desktop places windows itself, which is every Wayland compositor.
    pub fn set_position(&self, x: f32, y: f32) {
        self.act(|surface| surface.set_position(Point::new(CssPx(x), CssPx(y))));
    }

    /// Where the window is, when the desktop says.
    ///
    /// `None` on a desktop that does not tell a window where it has been put.
    pub fn position(&self) -> Option<Point<CssPx, Css>> {
        self.surface().and_then(|surface| surface.position())
    }

    /// Allows or forbids the user resizing the window.
    pub fn set_resizable(&self, resizable: bool) {
        self.act(|surface| surface.set_resizable(resizable));
    }

    /// Asks the desktop for a frame.
    ///
    /// macOS settles whether a frame carries a title bar when the window is created, so
    /// [`Decorations::NoTitleBar`] reaches a window that is already open as
    /// [`Decorations::Full`]. Ask for it in [`WindowOptions`](crate::windows::WindowOptions).
    pub fn set_decorations(&self, decorations: Decorations) {
        self.act(|surface| surface.set_decorations(decorations));
    }

    /// Maximises or restores the window.
    pub fn set_maximized(&self, maximized: bool) {
        self.act(|surface| surface.set_maximized(maximized));
    }

    /// Maximises the window, or restores it if it is already maximised.
    ///
    /// What a title bar's double-press does, and what its maximise button does.
    pub fn toggle_maximized(&self) {
        self.act(|surface| surface.set_maximized(!surface.is_maximized()));
    }

    /// Does what a double press on a title bar does on this desktop.
    ///
    /// That maximises the window or restores it, unless the desktop has a preference: macOS zooms,
    /// minimizes or does nothing, as the user set it in System Settings.
    pub fn title_bar_double_click(&self) {
        self.act(|surface| surface.title_bar_double_click());
    }

    /// Minimises the window.
    pub fn minimize(&self) {
        self.act(|surface| surface.set_minimized(true));
    }

    /// Puts the window full screen, or takes it out again.
    ///
    /// A desktop that cannot give a window a screen exclusively gives it a borderless one instead,
    /// which is what was asked for minus the part that could not be done.
    pub fn set_fullscreen(&self, mode: Option<FullscreenMode>) {
        self.act(|surface| surface.set_fullscreen(mode));
    }

    /// Asks for the window to sit at a level in the desktop's stacking order.
    ///
    /// Does nothing where a desktop does not let an application place itself in the stack.
    pub fn set_level(&self, level: WindowLevel) {
        self.act(|surface| surface.set_window_level(level));
    }

    /// Shows or hides the window.
    ///
    /// Hiding keeps the window, its graphics surface and its whole document, so showing it again
    /// costs one frame.
    pub fn set_visible(&self, visible: bool) {
        self.act(|surface| surface.set_visible(visible));
    }

    /// Sets the picture the desktop shows for this window.
    ///
    /// Does nothing where the desktop takes the icon from elsewhere — from the desktop entry on
    /// Wayland, from the bundle on macOS.
    pub fn set_icon(&self, icon: Option<WindowIcon>) {
        self.act(|surface| surface.set_icon(icon.as_ref()));
    }

    /// Overrides this window's light or dark preference, or returns it to the desktop's.
    pub fn set_theme(&self, theme: Option<ColorScheme>) {
        self.act(|surface| surface.set_theme(theme));
    }

    /// Asks the desktop to blur what shows through the transparent parts of the window, or to
    /// stop.
    ///
    /// Does nothing for a window that is not transparent, or where the desktop cannot blur a
    /// window.
    pub fn set_blur(&self, blur: bool) {
        self.act(|surface| surface.set_blur(blur));
    }

    /// Sets what the pointer looks like over this window.
    pub fn set_cursor(&self, cursor: CursorStyle) {
        self.act(|surface| surface.set_cursor(cursor));
    }

    /// Asks for the keyboard.
    ///
    /// A desktop is free to refuse: taking focus from what somebody is typing into is what
    /// focus-stealing prevention exists to stop.
    pub fn focus(&self) {
        self.act(|surface| surface.focus());
    }

    /// Asks the desktop to draw attention to this window, or stops asking.
    pub fn request_attention(&self, urgent: bool) {
        self.act(|surface| surface.request_attention(urgent));
    }

    /// Begins a desktop-driven move of the window.
    ///
    /// What a window drawing its own title bar calls from a pointer press on it: on a desktop that
    /// forbids a window from placing itself, this is the only way a self-drawn title bar can move
    /// one. See [`WindowHandle::move_drag_handler`], which is the form a view uses.
    pub fn begin_move_drag(&self) {
        self.act(|surface| {
            if surface.begin_move_drag().is_ok() {
                // The compositor has the pointer now and no release will arrive, so the frame has
                // to be told to end the press the router is still tracking.
                self.shared.drag_started.set(true);
            }
        });
    }

    /// Begins a desktop-driven resize of the window from one edge or corner.
    pub fn begin_resize_drag(&self, edge: ResizeEdge) {
        self.act(|surface| {
            if surface.begin_resize_drag(edge).is_ok() {
                self.shared.drag_started.set(true);
            }
        });
    }

    /// Closes the window.
    ///
    /// The window's own close callbacks are not consulted: those answer the *user* asking to close,
    /// and an application closing its own window has already decided.
    pub fn close(&self) {
        self.shared.commands.close(self.shared.id);
    }
}

impl core::fmt::Debug for WindowHandle {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("WindowHandle")
            .field("id", &self.shared.id)
            .field("open", &self.is_open())
            .finish_non_exhaustive()
    }
}

impl PartialEq for WindowHandle {
    /// Two handles are equal when they name the same window.
    fn eq(&self, other: &Self) -> bool {
        self.shared.id == other.shared.id
    }
}

impl Eq for WindowHandle {}
