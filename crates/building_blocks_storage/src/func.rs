//! Lattice map access traits implemented for functions and closures.
//!
//! This is particularly useful for sampling from signed-distance fields.
//!
//! ```
//! use building_blocks_core::prelude::*;
//! use building_blocks_storage::prelude::*;
//!
//! let sample_extent = Extent3i::from_min_and_max(PointN([-15; 3]), PointN([15; 3]));
//! let mut sampled_sphere = Array3::fill(sample_extent, 0.0);
//!
//! copy_extent(&sample_extent, &|p: &Point3i| (p.dot(p) - 10) as f32, &mut sampled_sphere);
//!```

use crate::{ForEachRef, Get, ReadExtent};

use building_blocks_core::prelude::*;

use core::iter::{once, Once};

impl<F, T, Coord> Get<Coord> for F
where
    F: Fn(Coord) -> T,
{
    type Data = T;

    fn get(&self, c: Coord) -> T {
        (self)(c)
    }
}

impl<'a, F, N, T> ForEachRef<N, PointN<N>> for F
where
    T: 'a,
    F: Fn(&PointN<N>) -> &'a T,
    PointN<N>: Copy,
    ExtentN<N>: IntegerExtent<N>,
{
    type Data = T;

    fn for_each_ref(&self, extent: &ExtentN<N>, mut f: impl FnMut(PointN<N>, &Self::Data)) {
        for p in extent.iter_points() {
            f(p, (self)(&p))
        }
    }
}

impl<'a, F, N, T> ReadExtent<'a, N> for F
where
    F: 'a + Fn(&PointN<N>) -> T,
    ExtentN<N>: Copy,
    PointN<N>: Point,
{
    type Src = &'a Self;
    type SrcIter = Once<(ExtentN<N>, Self::Src)>;

    fn read_extent(&'a self, extent: &ExtentN<N>) -> Self::SrcIter {
        once((*extent, self))
    }
}
