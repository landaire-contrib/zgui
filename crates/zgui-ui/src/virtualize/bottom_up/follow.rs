//! What a bottom-up list keeps in view, decided from what its port shows.

/// How far the port moves, in CSS pixels, before the list takes it as a move.
const MOTION: f64 = 0.5;

/// What a bottom-up list keeps in view as rows come, go and change height.
#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) enum Hold<K> {
    /// The end of the rows, which the list follows.
    End,
    /// The row with this key at the top of the port, until the reader scrolls.
    Row(K),
    /// The place the reader scrolled to.
    Free,
}

/// The heights of the content and of the port, in CSS pixels.
#[derive(Copy, Clone, PartialEq, Debug, Default)]
pub(crate) struct Extent {
    /// The height of everything in the container.
    pub(crate) content: f32,
    /// The height of the port.
    pub(crate) port: f32,
}

/// Where a held row is, in CSS pixels from the top of the container.
#[derive(Copy, Clone, PartialEq, Debug)]
pub(crate) struct Placed {
    /// The top of the row.
    pub(crate) row: f64,
    /// The top of the port.
    pub(crate) port: f64,
}

/// What the port shows at one moment.
#[derive(Copy, Clone, PartialEq, Debug)]
pub(crate) struct Sighting {
    /// How far the content goes below the bottom of the port, in CSS pixels.
    pub(crate) remaining: f64,
    /// The offset of the port less the scrolls the measurements asked for, in CSS pixels.
    pub(crate) drift: f64,
    /// Whether every scroll the measurements asked for has been seen.
    pub(crate) settled: bool,
    /// The heights of the content and of the port.
    pub(crate) extent: Extent,
    /// Whether the host has rows after the last row the list holds.
    pub(crate) newer: bool,
    /// Where the held row is, when a row is held and the list has it.
    pub(crate) held: Option<Placed>,
}

/// What the list does after a sighting.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub(crate) enum Step {
    /// Nothing.
    Stay,
    /// Scroll to the end of the content.
    ScrollToEnd,
    /// Scroll the held row to the top of the port.
    ScrollToRow,
}

/// A scroll to a held row that the list asked for.
#[derive(Copy, Clone, PartialEq, Debug)]
struct Asked {
    /// Where the row was, in CSS pixels from the top of the container.
    row: f64,
    /// The heights when the list asked.
    extent: Extent,
}

/// What a bottom-up list keeps in view, and what it saw last.
pub(crate) struct Follow<K> {
    /// What the list keeps in view.
    hold: Hold<K>,
    /// How near the end, in CSS pixels, counts as at the end.
    slack: f64,
    /// The drift the next sighting is compared with.
    drift: Option<f64>,
    /// The last scroll asked to bring the held row to the top, until the row is there.
    asked: Option<Asked>,
    /// The height of the port at the last sighting, and whether it showed the end.
    last: Option<(f32, bool)>,
}

impl<K> Follow<K>
where
    K: PartialEq,
{
    /// Follows the end, with `slack` CSS pixels that count as at the end.
    pub(crate) const fn new(slack: f64) -> Self {
        Self {
            hold: Hold::End,
            slack,
            drift: None,
            asked: None,
            last: None,
        }
    }

    /// What the list keeps in view.
    pub(crate) const fn hold(&self) -> &Hold<K> {
        &self.hold
    }

    /// Sets how near the end, in CSS pixels, counts as at the end.
    pub(crate) const fn set_slack(&mut self, slack: f64) {
        self.slack = slack;
    }

    /// Follows the end again.
    pub(crate) fn follow_end(&mut self) {
        self.hold = Hold::End;
        self.asked = None;
    }

    /// Holds the row with `key` at the top of the port.
    pub(crate) fn hold_row(&mut self, key: K) {
        self.hold = Hold::Row(key);
        self.asked = None;
    }

    /// Forgets where the port was, for a port that is not laid out.
    pub(crate) const fn forget_motion(&mut self) {
        self.drift = None;
        self.last = None;
    }

    /// Stops holding a row, because the reader took the port.
    pub(crate) fn let_go(&mut self) {
        if matches!(self.hold, Hold::Row(_)) {
            self.hold = Hold::Free;
            self.asked = None;
        }
    }

    /// Takes in what the port shows, and answers what to do about it.
    ///
    /// The list leaves the end when the content at the top of the port moves down on screen, which
    /// only the reader does. It follows the end again when the port shows the end, every scroll the
    /// measurements asked for has been seen, and the host has no newer rows. A held row that is
    /// away from the top of the port is scrolled to again when it or the heights moved since the
    /// last scroll to it, unless the port is at the end and the row is below its top. A port that
    /// shrinks while it shows the end, as one does when the keyboard opens under it, keeps the end
    /// in view and follows it.
    pub(crate) fn see(&mut self, sighting: Sighting) -> Step {
        let moved_up = self
            .drift
            .is_some_and(|before| sighting.drift < before - MOTION);
        // Small moves add up, so that a slow scroll is a move too.
        if self
            .drift
            .is_none_or(|before| (sighting.drift - before).abs() > MOTION)
        {
            self.drift = Some(sighting.drift);
        }
        let at_end = sighting.remaining <= self.slack;
        let shrank_at_end = self.last.is_some_and(|(port, shown)| {
            shown && f64::from(sighting.extent.port) < f64::from(port) - MOTION
        });
        // A scroll not yet seen can move the port off the end it showed.
        self.last = Some((sighting.extent.port, at_end && sighting.settled));
        // A row the list does not have yet is still waited for at the end.
        let waiting = matches!(self.hold, Hold::Row(_)) && sighting.held.is_none();
        if shrank_at_end && !sighting.newer && !waiting && self.hold != Hold::End {
            self.hold = Hold::End;
            self.asked = None;
            return Step::ScrollToEnd;
        }

        match &self.hold {
            Hold::End if at_end => Step::Stay,
            Hold::End if moved_up => {
                self.hold = Hold::Free;
                Step::Stay
            }
            Hold::End => Step::ScrollToEnd,
            Hold::Free => {
                if at_end && sighting.settled && !sighting.newer {
                    self.hold = Hold::End;
                }
                Step::Stay
            }
            Hold::Row(_) => {
                let Some(placed) = sighting.held else {
                    return Step::Stay;
                };
                // A row below the top of a port that cannot go further down is as high as it goes.
                let reached = (placed.row - placed.port).abs() <= MOTION
                    || (placed.row > placed.port && sighting.remaining <= MOTION);
                if reached {
                    self.asked = None;
                    return Step::Stay;
                }

                let asked = Asked {
                    row: placed.row,
                    extent: sighting.extent,
                };
                if self.asked == Some(asked) {
                    return Step::Stay;
                }
                self.asked = Some(asked);
                Step::ScrollToRow
            }
        }
    }

    /// Whether the port shows the newest rows: it follows the end or shows it, and the host has no
    /// newer rows.
    pub(crate) fn at_end(&self, sighting: &Sighting) -> bool {
        !sighting.newer && (self.hold == Hold::End || sighting.remaining <= self.slack)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A port 400px tall over content 2 000px tall, scrolled to `offset`, with no rows held.
    fn at(offset: f64) -> Sighting {
        Sighting {
            remaining: 1_600.0 - offset,
            drift: offset,
            settled: true,
            extent: Extent {
                content: 2_000.0,
                port: 400.0,
            },
            newer: false,
            held: None,
        }
    }

    #[test]
    fn content_that_grows_below_the_port_is_followed() {
        let mut follow = Follow::<u32>::new(4.0);
        assert_eq!(follow.see(at(1_600.0)), Step::Stay);

        let grown = Sighting {
            remaining: 48.0,
            ..at(1_600.0)
        };
        assert_eq!(follow.see(grown), Step::ScrollToEnd);
        assert_eq!(follow.hold(), &Hold::End);
        assert!(
            follow.at_end(&grown),
            "a list that follows the end is at the end"
        );
    }

    #[test]
    fn a_move_up_leaves_the_end_and_a_return_to_it_follows_again() {
        let mut follow = Follow::<u32>::new(4.0);
        follow.see(at(1_600.0));

        // The first frame of the move stays inside the slack.
        assert_eq!(follow.see(at(1_598.0)), Step::Stay);
        assert_eq!(follow.hold(), &Hold::End);
        assert_eq!(follow.see(at(1_500.0)), Step::Stay);
        assert_eq!(follow.hold(), &Hold::Free);
        assert!(!follow.at_end(&at(1_500.0)));

        assert_eq!(follow.see(at(1_597.0)), Step::Stay);
        assert_eq!(follow.hold(), &Hold::End);
    }

    #[test]
    fn a_scroll_that_the_measurements_asked_for_does_not_leave_the_end() {
        let mut follow = Follow::<u32>::new(4.0);
        follow.see(at(1_600.0));

        // A row above the port shrank by 20px, and a row below it grew by 48px.
        let shifted = Sighting {
            remaining: 48.0,
            drift: 1_600.0,
            ..at(1_580.0)
        };
        assert_eq!(follow.see(shifted), Step::ScrollToEnd);
        assert_eq!(follow.hold(), &Hold::End);
    }

    #[test]
    fn a_slow_move_up_leaves_the_end() {
        let mut follow = Follow::<u32>::new(4.0);
        follow.see(at(1_600.0));
        let mut offset = 1_600.0;
        while offset > 1_590.0 {
            offset -= 0.25;
            follow.see(at(offset));
        }
        assert_eq!(follow.hold(), &Hold::Free);
    }

    #[test]
    fn an_end_reached_before_a_scroll_the_measurements_asked_for_is_not_followed() {
        let mut follow = Follow::<u32>::new(4.0);
        follow.see(at(1_600.0));
        follow.see(at(1_500.0));

        // A row above the port shrank, and the content is shorter before the port moves up.
        let shrunk = Sighting {
            settled: false,
            ..at(1_598.0)
        };
        assert_eq!(follow.see(shrunk), Step::Stay);
        assert_eq!(follow.hold(), &Hold::Free);
    }

    #[test]
    fn a_return_to_the_end_on_request_follows_it() {
        let mut follow = Follow::<u32>::new(4.0);
        follow.see(at(1_600.0));
        follow.see(at(500.0));
        assert_eq!(follow.hold(), &Hold::Free);

        follow.follow_end();
        assert_eq!(follow.see(at(500.0)), Step::ScrollToEnd);
    }

    #[test]
    fn newer_rows_keep_the_end_from_being_followed() {
        let mut follow = Follow::<u32>::new(4.0);
        follow.see(at(1_600.0));
        follow.see(at(1_000.0));

        let newer = Sighting {
            newer: true,
            ..at(1_600.0)
        };
        assert_eq!(follow.see(newer), Step::Stay);
        assert_eq!(follow.hold(), &Hold::Free);
        assert!(!follow.at_end(&newer));
    }

    #[test]
    fn a_held_row_is_scrolled_to_once_for_each_change_and_let_go_by_the_reader() {
        let mut follow = Follow::new(4.0);
        follow.hold_row(7_u32);
        let away = Sighting {
            held: Some(Placed {
                row: 900.0,
                port: 1_600.0,
            }),
            ..at(1_600.0)
        };
        assert_eq!(follow.see(away), Step::ScrollToRow);
        assert_eq!(
            follow.see(away),
            Step::Stay,
            "the same scroll is not asked twice"
        );

        let taller = Sighting {
            extent: Extent {
                content: 2_100.0,
                port: 400.0,
            },
            ..away
        };
        assert_eq!(follow.see(taller), Step::ScrollToRow);

        let placed = Sighting {
            held: Some(Placed {
                row: 900.0,
                port: 900.0,
            }),
            ..at(900.0)
        };
        assert_eq!(follow.see(placed), Step::Stay);
        assert_eq!(follow.hold(), &Hold::Row(7));

        follow.let_go();
        assert_eq!(follow.hold(), &Hold::Free);
        assert_eq!(follow.see(at(700.0)), Step::Stay);
    }

    #[test]
    fn a_held_row_below_the_top_of_a_port_at_the_end_is_placed() {
        let mut follow = Follow::new(4.0);
        follow.hold_row(7_u32);
        let short = Sighting {
            remaining: 0.0,
            extent: Extent {
                content: 400.0,
                port: 400.0,
            },
            held: Some(Placed {
                row: 208.0,
                port: 0.0,
            }),
            ..at(0.0)
        };
        assert_eq!(follow.see(short), Step::Stay);
        assert_eq!(follow.hold(), &Hold::Row(7), "the row is still held");

        // The rows below it grew, so the port can go further down.
        let grown = Sighting {
            remaining: 300.0,
            extent: Extent {
                content: 700.0,
                port: 400.0,
            },
            ..short
        };
        assert_eq!(follow.see(grown), Step::ScrollToRow);
    }

    #[test]
    fn a_held_row_the_list_does_not_have_is_waited_for() {
        let mut follow = Follow::new(4.0);
        follow.hold_row(7_u32);
        assert_eq!(follow.see(at(1_600.0)), Step::Stay);
        assert_eq!(follow.hold(), &Hold::Row(7));
        assert!(
            follow.at_end(&at(1_600.0)),
            "the port shows the end while it waits"
        );
    }

    #[test]
    fn a_port_that_shrinks_while_it_waits_for_a_held_row_still_waits() {
        let mut follow = Follow::new(4.0);
        follow.hold_row(7_u32);
        follow.see(at(1_600.0));
        let shrunk = Sighting {
            remaining: 300.0,
            extent: Extent {
                content: 2_000.0,
                port: 100.0,
            },
            ..at(1_600.0)
        };
        assert_eq!(follow.see(shrunk), Step::Stay);
        assert_eq!(follow.hold(), &Hold::Row(7));
    }

    #[test]
    fn a_port_that_shrinks_before_a_scroll_is_seen_is_not_at_the_end() {
        let mut follow = Follow::new(4.0);
        follow.hold_row(7_u32);
        let pending = Sighting {
            remaining: 0.0,
            settled: false,
            held: Some(Placed {
                row: 1_800.0,
                port: 1_600.0,
            }),
            ..at(1_600.0)
        };
        follow.see(pending);
        let shrunk = Sighting {
            remaining: 300.0,
            extent: Extent {
                content: 2_000.0,
                port: 100.0,
            },
            ..pending
        };
        assert_ne!(follow.see(shrunk), Step::ScrollToEnd);
        assert_eq!(follow.hold(), &Hold::Row(7));
    }

    #[test]
    fn a_port_that_shrinks_at_the_end_keeps_the_end_in_view() {
        let mut follow = Follow::new(4.0);
        follow.hold_row(7_u32);
        let held = Sighting {
            remaining: 0.0,
            held: Some(Placed {
                row: 1_800.0,
                port: 1_600.0,
            }),
            ..at(1_600.0)
        };
        assert_eq!(follow.see(held), Step::Stay);
        assert_eq!(follow.hold(), &Hold::Row(7));

        // The keyboard opened under the port and took 300px of it.
        let shrunk = Sighting {
            remaining: 300.0,
            extent: Extent {
                content: 2_000.0,
                port: 100.0,
            },
            ..held
        };
        assert_eq!(follow.see(shrunk), Step::ScrollToEnd);
        assert_eq!(
            follow.hold(),
            &Hold::End,
            "the list follows the end from there"
        );
    }

    #[test]
    fn a_port_that_shrinks_away_from_the_end_stays_where_it_is() {
        let mut follow = Follow::<u32>::new(4.0);
        follow.see(at(1_600.0));
        follow.see(at(1_000.0));
        let shrunk = Sighting {
            remaining: 900.0,
            extent: Extent {
                content: 2_000.0,
                port: 100.0,
            },
            ..at(1_000.0)
        };
        assert_eq!(follow.see(shrunk), Step::Stay);
        assert_eq!(follow.hold(), &Hold::Free);
    }
}
