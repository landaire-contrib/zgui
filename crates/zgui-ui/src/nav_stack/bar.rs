//! The bar row of one page, and how its back button and title share the width.

use zgui::component;
use zgui::geom::DevicePx;
use zgui::prelude::*;
use zgui::reactive::LocalStorage;
use zgui::reactive::Memo;
use zgui::reactive::UnsyncCallback;
use zgui::view;
use zgui::view::AttrName;
use zgui::view::CustomPropertyName;
use zgui::vocab::SharedString;
use zgui_ui_icons::prelude::*;
use zgui_ui_icons::set::chevron::CHEVRON_LEFT;

use crate::nav_stack::page::PageBar;
use crate::nav_stack::path::NavCore;
use crate::nav_stack::path::PageId;

/// What the back button shows beside its chevron.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub(crate) enum BackLabel {
    /// The title of the page below.
    Full,
    /// The short label, "Back" or its translation.
    Short,
    /// No text.
    Chevron,
}

/// The widths of the back button with each label, in device pixels.
#[derive(Copy, Clone, PartialEq, Debug)]
pub(crate) struct BackWidths {
    /// With the title of the page below.
    pub(crate) full: DevicePx,
    /// With the short label.
    pub(crate) short: DevicePx,
    /// With the chevron only.
    pub(crate) chevron: DevicePx,
}

impl BackWidths {
    fn of(self, label: BackLabel) -> DevicePx {
        match label {
            BackLabel::Full => self.full,
            BackLabel::Short => self.short,
            BackLabel::Chevron => self.chevron,
        }
    }
}

/// The natural widths of what shares one bar row, in device pixels.
#[derive(Copy, Clone, PartialEq, Debug)]
pub(crate) struct BarWidths {
    /// The content width of the row.
    pub(crate) row: DevicePx,
    /// The space between two neighbors.
    pub(crate) gap: DevicePx,
    /// The back button, when the page offers back.
    pub(crate) back: Option<BackWidths>,
    /// The leading controls after the back button.
    pub(crate) leading: DevicePx,
    /// The trailing controls.
    pub(crate) trailing: DevicePx,
    /// The title. Zero while the bar hides the title.
    pub(crate) title: DevicePx,
}

impl BarWidths {
    /// Lays the row out as `UINavigationBar` does.
    ///
    /// The leading and trailing controls keep their widths. The title stays centered on the row
    /// when it fits there. The back button shows the title of the page below while that fits
    /// beside the centered title, then "Back", then the chevron only. The title moves off center
    /// before the back button loses "Back", and it truncates only when the chevron alone leaves
    /// too little space.
    pub(crate) fn layout(self) -> BarLayout {
        const WITH_BACK: [(BackLabel, Centering); 5] = [
            (BackLabel::Full, Centering::Required),
            (BackLabel::Short, Centering::Required),
            (BackLabel::Short, Centering::Preferred),
            (BackLabel::Chevron, Centering::Required),
            (BackLabel::Chevron, Centering::Preferred),
        ];
        const WITHOUT_BACK: [Centering; 2] = [Centering::Required, Centering::Preferred];

        let end = self.trailing_edge();
        let fitted = |back: Option<BackLabel>, centering: Centering| {
            self.place_whole(self.leading_edge(back), end, centering)
                .map(|title| BarLayout { back, title })
        };
        let found = if self.back.is_some() {
            WITH_BACK
                .iter()
                .find_map(|&(label, centering)| fitted(Some(label), centering))
        } else {
            WITHOUT_BACK
                .iter()
                .find_map(|&centering| fitted(None, centering))
        };
        found.unwrap_or_else(|| {
            let back = self.back.map(|_| BackLabel::Chevron);
            let left = self.leading_edge(back);
            BarLayout {
                back,
                title: TitleSpan {
                    left,
                    right: self.row - if end > left { end } else { left },
                },
            }
        })
    }

    /// Where the whole title goes between `start` and `end`, if it fits as `centering` asks.
    fn place_whole(
        self,
        start: DevicePx,
        end: DevicePx,
        centering: Centering,
    ) -> Option<TitleSpan> {
        let centered = (self.row - self.title) / 2.0;
        let left = match centering {
            Centering::Required => {
                (centered >= start && centered + self.title <= end).then_some(centered)?
            }
            Centering::Preferred => {
                if end - start < self.title {
                    return None;
                }
                if centered < start {
                    start
                } else if centered + self.title > end {
                    end - self.title
                } else {
                    centered
                }
            }
        };
        Some(TitleSpan {
            left,
            right: self.row - left - self.title,
        })
    }

    /// Where the title can start: after the back button with `label`, the leading controls and a
    /// gap.
    fn leading_edge(self, label: Option<BackLabel>) -> DevicePx {
        let back = match (self.back, label) {
            (Some(widths), Some(label)) => widths.of(label),
            _ => DevicePx::ZERO,
        };
        let occupied = match (back > DevicePx::ZERO, self.leading > DevicePx::ZERO) {
            (true, true) => back + self.gap + self.leading,
            (true, false) => back,
            (false, _) => self.leading,
        };
        if occupied > DevicePx::ZERO {
            occupied + self.gap
        } else {
            DevicePx::ZERO
        }
    }

    /// Where the title must end: before the trailing controls and a gap.
    fn trailing_edge(self) -> DevicePx {
        if self.trailing > DevicePx::ZERO {
            self.row - self.trailing - self.gap
        } else {
            self.row
        }
    }
}

/// Where the title goes, in device pixels from the edges of the row's content. The title truncates
/// when the space between is narrower than the title.
#[derive(Copy, Clone, PartialEq, Debug)]
pub(crate) struct TitleSpan {
    /// From the leading edge of the content to the leading edge of the title.
    pub(crate) left: DevicePx,
    /// From the trailing edge of the title to the trailing edge of the content.
    pub(crate) right: DevicePx,
}

/// How a bar row lays out its back button and title.
#[derive(Copy, Clone, PartialEq, Debug)]
pub(crate) struct BarLayout {
    /// The label of the back button, when the page offers back.
    pub(crate) back: Option<BackLabel>,
    /// Where the title goes.
    pub(crate) title: TitleSpan,
}

/// Whether a title must stay centered for a layout to be taken.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
enum Centering {
    Required,
    Preferred,
}

/// A distance from an edge of the row's content, in device pixels, as a CSS length from the same
/// edge of the row's padding box.
fn css_inset(length: DevicePx, padding: DevicePx, scale: f32) -> String {
    format!("{}px", (padding + length).0 / scale)
}

/// The bar of one page: back button or leading controls, title, trailing controls.
///
/// A probe that is never drawn measures the back button with each label and the title, so the
/// row can choose a layout before it paints. The probe carries the classes of the drawn elements,
/// so a style sheet that restyles them restyles the probe too.
#[component]
pub(crate) fn NavBarRow(
    /// The stack.
    core: NavCore,
    /// Which page.
    id: PageId,
    /// What the page declared.
    bar: PageBar,
    /// Goes back.
    back: UnsyncCallback<()>,
    /// What the back button says when the title of the page below does not fit.
    back_label: Signal<String, LocalStorage>,
) -> impl IntoView {
    let place = Signal::derive_local(move || core.place(id));
    // A page below with no title is named by the short label, so the button always has a name.
    let back_title = move || {
        let title = core.bar_below(id).map(PageBar::title).unwrap_or_default();
        if title.is_empty() {
            back_label.get()
        } else {
            title
        }
    };
    let offers_back = move || core.has_page_below(id) && !bar.is_back_blocked();

    let row = NodeRef::new();
    let extra = NodeRef::new();
    let trailing_box = NodeRef::new();
    let probe_full = NodeRef::new();
    let probe_short = NodeRef::new();
    let probe_chevron = NodeRef::new();
    let probe_title = NodeRef::new();
    let probe_view = NodeRef::new();
    let probe_gap = NodeRef::new();
    // Content sizes are taken before transforms, so a scaled ancestor leaves them as laid out.
    // Each reads zero until it is first measured.
    let row_size = row.observe_content_size();
    let extra_size = extra.observe_content_size();
    let trailing_size = trailing_box.observe_content_size();
    let full_size = probe_full.observe_content_size();
    let short_size = probe_short.observe_content_size();
    let chevron_size = probe_chevron.observe_content_size();
    let title_size = probe_title.observe_content_size();
    let view_size = probe_view.observe_content_size();
    let gap_size = probe_gap.observe_content_size();

    let layout = Memo::new(move |_| {
        let row = row_size.get().width;
        let chevron = chevron_size.get().width;
        // The row and the chevron always have a width once laid out.
        if row <= DevicePx::ZERO || chevron <= DevicePx::ZERO {
            return None;
        }

        let back = offers_back().then(|| BackWidths {
            full: full_size.get().width,
            short: short_size.get().width,
            chevron,
        });
        let title = if !bar.shows_title() {
            DevicePx::ZERO
        } else if bar.title_view().is_some() {
            view_size.get().width
        } else {
            title_size.get().width
        };
        let widths = BarWidths {
            row,
            gap: gap_size.get().width,
            back,
            leading: extra_size.get().width,
            trailing: trailing_size.get().width,
            title,
        };
        Some(widths.layout())
    });

    let shown_label = move || {
        let label = layout
            .get()
            .and_then(|layout| layout.back)
            .unwrap_or(BackLabel::Full);
        match label {
            BackLabel::Full => Some(back_title()),
            BackLabel::Short => Some(back_label.get()),
            BackLabel::Chevron => None,
        }
    };
    let back_text =
        move || shown_label().map(|label| view! { text(class = "zui-nav-back__title") {{label}} });
    let back_button = move || {
        if !offers_back() {
            return None;
        }

        Some(view! {
            control(
                class = "zui-nav-back",
                tabindex = {Focus::Sequential},
                on:click = move |_| back.run(()),
                {..Attrs::new().a11y_from(
                    A11yBinding::new(Role::Button).label(move || SharedString::from(back_title()))
                )}
            ) {
                {view! { Icon(icon = CHEVRON_LEFT) }}
                {back_text}
            }
        })
    };
    let leading = move || bar.leading().map(|leading| leading.view());
    let trailing = move || bar.trailing().map(|trailing| trailing.view());

    let inset = move |edge: fn(TitleSpan) -> DevicePx| {
        let layout = layout.get()?;
        Some(css_inset(
            edge(layout.title),
            gap_size.get().width,
            row.scale(),
        ))
    };
    let own = Attrs::new()
        .attribute(AttrName::new("data-place"), move || {
            Some(place.get().name().to_owned())
        })
        .attribute(AttrName::new("inert"), move || {
            (!place.get().is_active()).then(String::new)
        })
        .custom_property(CustomPropertyName::new("zui-nav-title-left"), move || {
            inset(|span| span.left)
        })
        .custom_property(CustomPropertyName::new("zui-nav-title-right"), move || {
            inset(|span| span.right)
        })
        .a11y_from(A11yBinding::unspecified().hidden(move || !place.get().is_active()));
    let probe = Attrs::new().attribute(AttrName::new("inert"), || Some(String::new()));

    view! {
        row(class = "zui-nav-bar__row", node_ref = row, {..own}) {
            box(class = "zui-nav-bar__leading") {
                {back_button}
                box(class = "zui-nav-bar__extra", node_ref = extra) {{leading}}
            }
            {move || match bar.title_view() {
                Some(view) => AnyView::new(view! {
                    box(
                        class = "zui-nav-bar__title zui-nav-bar__title-view",
                        {..Attrs::new()
                            .attribute(AttrName::new("data-hidden"), move || {
                                (!bar.shows_title()).then(String::new)
                            })
                            .attribute(AttrName::new("inert"), move || {
                                (!bar.shows_title()).then(String::new)
                            })
                            .a11y_from(A11yBinding::unspecified().hidden(move || !bar.shows_title()))}
                    ) {
                        {view.view()}
                    }
                }),
                None => AnyView::new(view! {
                    text(
                        class = "zui-nav-bar__title",
                        {..Attrs::new()
                            .attribute(AttrName::new("data-hidden"), move || {
                                (!bar.shows_title()).then(String::new)
                            })
                            .a11y_from(
                                A11yBinding::new(Role::Heading)
                                    .level(1_usize)
                                    .hidden(move || !bar.shows_title())
                            )}
                    ) {
                        {move || bar.title()}
                    }
                }),
            }}
            box(class = "zui-nav-bar__trailing", node_ref = trailing_box) {{trailing}}
            row(class = "zui-nav-bar__probe", a11y:hidden = true, {..probe}) {
                row(class = "zui-nav-back", node_ref = probe_full) {
                    Icon(icon = CHEVRON_LEFT)
                    text(class = "zui-nav-back__title") {{back_title}}
                }
                row(class = "zui-nav-back", node_ref = probe_short) {
                    Icon(icon = CHEVRON_LEFT)
                    text(class = "zui-nav-back__title") {{move || back_label.get()}}
                }
                row(class = "zui-nav-back", node_ref = probe_chevron) {
                    Icon(icon = CHEVRON_LEFT)
                }
                text(class = "zui-nav-bar__title", node_ref = probe_title) {
                    {move || bar.title()}
                }
                box(class = "zui-nav-bar__probe-view", node_ref = probe_view) {
                    {move || bar.title_view().map(|view| view.view())}
                }
                box(class = "zui-nav-bar__probe-gap", node_ref = probe_gap)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use zgui::geom::DevicePx;

    use super::BackLabel;
    use super::BackWidths;
    use super::BarLayout;
    use super::BarWidths;
    use super::TitleSpan;

    /// A row with a back button whose full label is 150 pixels wide.
    fn bar(row: f32, title: f32) -> BarWidths {
        BarWidths {
            row: DevicePx(row),
            gap: DevicePx(8.0),
            back: Some(BackWidths {
                full: DevicePx(150.0),
                short: DevicePx(60.0),
                chevron: DevicePx(44.0),
            }),
            leading: DevicePx::ZERO,
            trailing: DevicePx::ZERO,
            title: DevicePx(title),
        }
    }

    fn laid_out(back: Option<BackLabel>, left: f32, right: f32) -> BarLayout {
        BarLayout {
            back,
            title: TitleSpan {
                left: DevicePx(left),
                right: DevicePx(right),
            },
        }
    }

    #[test]
    fn the_full_back_title_shows_while_it_fits_beside_the_centered_title() {
        assert_eq!(
            bar(400.0, 80.0).layout(),
            laid_out(Some(BackLabel::Full), 160.0, 160.0)
        );
    }

    #[test]
    fn the_back_title_shortens_before_the_title_moves_off_center() {
        assert_eq!(
            bar(400.0, 200.0).layout(),
            laid_out(Some(BackLabel::Short), 100.0, 100.0)
        );
    }

    #[test]
    fn the_title_moves_off_center_before_back_loses_its_label() {
        assert_eq!(
            bar(400.0, 300.0).layout(),
            laid_out(Some(BackLabel::Short), 68.0, 32.0)
        );
    }

    #[test]
    fn only_the_chevron_shows_when_back_leaves_too_little_space() {
        assert_eq!(
            bar(400.0, 340.0).layout(),
            laid_out(Some(BackLabel::Chevron), 52.0, 8.0)
        );
    }

    #[test]
    fn the_title_truncates_only_after_back_is_a_chevron() {
        assert_eq!(
            bar(400.0, 380.0).layout(),
            laid_out(Some(BackLabel::Chevron), 52.0, 0.0)
        );
    }

    #[test]
    fn the_trailing_controls_keep_their_width_before_the_back_title() {
        let crowded = BarWidths {
            trailing: DevicePx(200.0),
            ..bar(400.0, 80.0)
        };
        assert_eq!(
            crowded.layout(),
            laid_out(Some(BackLabel::Short), 112.0, 208.0)
        );
    }

    #[test]
    fn a_page_without_back_centers_its_title_between_its_controls() {
        let root = BarWidths {
            back: None,
            leading: DevicePx(60.0),
            trailing: DevicePx(30.0),
            ..bar(400.0, 200.0)
        };
        assert_eq!(root.layout(), laid_out(None, 100.0, 100.0));
    }
}
