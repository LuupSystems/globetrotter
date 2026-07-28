//! Extension traits used by the translation model.

/// Iterator extension traits.
pub mod iter {
    /// Fallible [`Iterator::unzip`] over iterators of `Result<(A, B), E>`.
    pub trait TryUnzipExt: Iterator {
        /// Unzips successful pairs into two collections.
        ///
        /// On success, returns `(left, right)`. The first error stops iteration
        /// and is returned without consuming later items.
        ///
        /// Like [`Iterator::unzip`], the destination collections may be any
        /// types implementing `Default + Extend`.
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
            // Generic destination collections expose no common capacity API.
            let mut left = C1::default();
            let mut right = C2::default();

            for item in self {
                // Stop at the first source error.
                let (a, b) = item?;
                left.extend(std::iter::once(a));
                right.extend(std::iter::once(b));
            }
            Ok((left, right))
        }

        /// Collects both sides into `Vec`s, stopping at the first error.
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
