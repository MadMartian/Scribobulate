//! Where the find cursor's current preview hit sits, held so that it survives the
//! preview buffer being rewritten around it.
//!
//! A preview buffer offset is the wrong thing to hold across a fold splice. The splice
//! deletes and rewrites one block's region in the live buffer, so every offset below
//! it moves by the region's length change while the source does not move at all (the
//! Document-Reference CAM's class A′). A cursor held as a buffer offset then names a
//! different place in the rebuilt list, and the readout falls back to a bare count.
//!
//! So the position is held against the render's source map instead: the cleaned source
//! byte of the waypoint at or before the hit, plus how many buffer characters the hit
//! sits past that waypoint. The splice and a re-render both install a source map built
//! from a full parse, so the same waypoint is found in the new map and the hit is put
//! back the same distance past it. Text inside one waypoint's run is rendered by one
//! event and does not change under a splice elsewhere.
//!
//! Pure over `(buffer_char, source_byte)` maps, so it is tested without a display.

/// A preview position expressed against the render's source map. See the module doc.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct HitAnchor {
    /// Cleaned source byte of the first waypoint of the run holding the hit, or `None`
    /// when the hit sits before every waypoint (counted from the buffer start).
    waypoint: Option<usize>,
    /// Buffer characters from that waypoint to the hit.
    past: i32,
}

impl HitAnchor {
    /// Anchor `buf_char` in `map`, a render's source map sorted by buffer offset.
    ///
    /// The waypoint is the FIRST one in the run sharing the at-or-before waypoint's
    /// source byte, because [`resolve`](Self::resolve) finds a source byte by its first
    /// entry. Two waypoints with one source byte are adjacent in a well-formed map.
    pub(crate) fn at(map: &[(i32, usize)], buf_char: i32) -> Self {
        let Some(j) = map
            .partition_point(|&(bc, _)| bc <= buf_char)
            .checked_sub(1)
        else {
            return Self {
                waypoint: None,
                past: buf_char,
            };
        };
        let (_, src) = map[j];
        let mut k = j;
        while k > 0 && map[k - 1].1 == src {
            k -= 1;
        }
        let (base, _) = map[k];
        Self {
            waypoint: Some(src),
            past: buf_char - base,
        }
    }

    /// The buffer offset this anchor names in `map`, a source map of the render now
    /// installed.
    ///
    /// A waypoint the new render does not have (its block was just collapsed, say) has
    /// no run to measure `past` along, so the answer is the nearest waypoint at or before
    /// its source byte. Find then resumes after that point, which is the "first match
    /// after where it was" TDD 11.12 asks for.
    pub(crate) fn resolve(self, map: &[(i32, usize)]) -> i32 {
        let Some(src) = self.waypoint else {
            return self.past;
        };
        if let Some(&(base, _)) = map.iter().find(|&&(_, sb)| sb == src) {
            return base + self.past;
        }
        map.iter()
            .filter(|&&(_, sb)| sb <= src)
            .map(|&(bc, _)| bc)
            .max()
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::HitAnchor;

    /// A render with three blocks: waypoints at buffer 0, 20 and 40, source 0, 30, 60.
    const BEFORE: &[(i32, usize)] = &[(0, 0), (20, 30), (40, 60), (55, 80)];
    /// The same source after a splice inserted 100 characters into the first block.
    const AFTER: &[(i32, usize)] = &[(0, 0), (5, 10), (120, 30), (140, 60), (155, 80)];

    #[test]
    fn a_hit_below_a_splice_moves_with_its_text() {
        let anchor = HitAnchor::at(BEFORE, 47);
        assert_eq!(anchor.resolve(BEFORE), 47, "unchanged render, same offset");
        assert_eq!(
            anchor.resolve(AFTER),
            147,
            "100 characters inserted above it"
        );
    }

    #[test]
    fn a_hit_whose_waypoint_is_gone_resumes_at_the_nearest_one_before_it() {
        let anchor = HitAnchor::at(AFTER, 7);
        assert_eq!(anchor.resolve(BEFORE), 0);
    }

    #[test]
    fn a_hit_before_every_waypoint_is_counted_from_the_buffer_start() {
        let anchor = HitAnchor::at(&[(10, 4)], 3);
        assert_eq!(anchor.resolve(&[(90, 4)]), 3);
    }

    /// Two waypoints sharing a source byte: the anchor is measured from the first, the
    /// one `resolve` finds, so a hit past the second still comes back where it was.
    #[test]
    fn a_shared_source_byte_is_measured_from_its_first_waypoint() {
        let map: &[(i32, usize)] = &[(0, 0), (10, 30), (12, 30), (30, 50)];
        let anchor = HitAnchor::at(map, 20);
        assert_eq!(anchor.resolve(map), 20);
        let shifted: &[(i32, usize)] = &[(0, 0), (60, 30), (62, 30), (80, 50)];
        assert_eq!(anchor.resolve(shifted), 70);
    }
}
