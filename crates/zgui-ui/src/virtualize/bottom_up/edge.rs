//! When the reader comes near one end of the rows a bottom-up list holds.

/// One end of the rows a list holds, and whether the host was told of the reader's approach.
pub(crate) struct Edge<K> {
    /// Whether the next approach is told.
    armed: bool,
    /// The key of the row at this end, as of the last sighting.
    row: Option<K>,
}

impl<K> Edge<K>
where
    K: PartialEq,
{
    /// An end that no one has come near.
    pub(crate) const fn new() -> Self {
        Self {
            armed: true,
            row: None,
        }
    }

    /// Takes in the key of the row at this end, and whether the port is near it. Answers whether
    /// to tell the host.
    ///
    /// The host is told once for each approach. The port that leaves the end, or a row at the end
    /// with another key, starts a new approach. A list with no rows tells nothing.
    pub(crate) fn see(&mut self, row: Option<K>, near: bool) -> bool {
        if self.row != row {
            self.row = row;
            self.armed = true;
        }
        if !near {
            self.armed = true;
            return false;
        }

        let tell = self.armed && self.row.is_some();
        if tell {
            self.armed = false;
        }
        tell
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_approach_is_told_once() {
        let mut edge = Edge::new();
        assert!(!edge.see(Some(1_u32), false));
        assert!(edge.see(Some(1), true));
        assert!(!edge.see(Some(1), true), "the port is still near");

        assert!(!edge.see(Some(1), false));
        assert!(edge.see(Some(1), true), "the port left and came back");
    }

    #[test]
    fn a_new_row_at_the_end_is_a_new_approach() {
        let mut edge = Edge::new();
        assert!(edge.see(Some(1_u32), true));
        assert!(
            edge.see(Some(0), true),
            "a page came in and the port is still near"
        );
        assert!(!edge.see(Some(0), true));
    }

    #[test]
    fn a_list_with_no_rows_tells_nothing() {
        let mut edge = Edge::<u32>::new();
        assert!(!edge.see(None, true));
        assert!(edge.see(Some(4), true));
    }
}
