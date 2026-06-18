/// Iterator extension traits.
pub mod iter {
    /// Fallible [`Iterator::unzip`] over iterators of `Result<(A, B), E>`.
    pub trait TryUnzipExt: Iterator {
        /// Try to unzip an iterator of `Result<(A, B), E>` into two collections.
        ///
        /// On success, returns `(left, right)`; on the first `Err(e)`, returns `Err(e)`
        /// and stops consuming the iterator.
        ///
        /// This mirrors `Iterator::unzip` in that the destination collections can be
        /// any types implementing `Default + Extend`.
        ///
        /// # Errors
        ///
        /// Propagates the first error produced by the underlying iterator.
        fn try_unzip<A, B, C1, C2, E>(self) -> Result<(C1, C2), E>
        where
            Self: Sized + Iterator<Item = Result<(A, B), E>>,
            C1: Default + Extend<A>,
            C2: Default + Extend<B>,
        {
            // we can’t generally reserve without specialization, so we keep it simple
            let mut left = C1::default();
            let mut right = C2::default();

            for item in self {
                let (a, b) = item?; // short-circuit on error
                left.extend(std::iter::once(a));
                right.extend(std::iter::once(b));
            }
            Ok((left, right))
        }

        /// Convenience helper that always returns `Vec`s.
        ///
        /// # Errors
        ///
        /// Propagates the first error produced by the underlying iterator.
        fn try_unzip_vec<A, B, E>(self) -> Result<(Vec<A>, Vec<B>), E>
        where
            Self: Sized + Iterator<Item = Result<(A, B), E>>,
        {
            self.try_unzip::<A, B, Vec<A>, Vec<B>, E>()
        }
    }

    impl<I: Iterator> TryUnzipExt for I {}
}
