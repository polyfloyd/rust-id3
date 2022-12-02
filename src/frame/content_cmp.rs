use std::borrow::Cow;

/// used to express that some content should or should not be compared
pub enum ContentCmp<'a> {
    Comparable(Vec<Cow<'a, [u8]>>),
    /// used to mark frames to be always different (for example for unknown frames)
    Incomparable,
    /// used to mark frames as identical regardless of their content
    /// (for example for frames which require an unique id)
    /// <br>(Note: both values must be <code>Same</code> or else they would not be equal)
    Same
}

impl <'a>PartialEq for ContentCmp<'a> {
    fn eq(&self, other: &Self) -> bool {
        use ContentCmp::*;

        match (self, other) {
            (Comparable(c1), Comparable(c2)) => c1 == c2,
            (Same, Same) => true,
            _ => false
        }
    }
}
