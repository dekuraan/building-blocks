//! A lattice map that applies a transformation to another lattice map.
//!
//! As an example use case, say you have a large lattice map that can store various types of voxels,
//! and each type of voxel has some associated data. If that data is even moderately sized, it could
//! take up a lot of space by storing copies at every point of the lattice.
//!
//! Instead, you can store that data in a "palette" array, and store indices into that array as your
//! voxel data.
//!
//! ```
//! use building_blocks_core::prelude::*;
//! use building_blocks_storage::prelude::*;
//!
//! struct BigData([u8; 9001]);
//!
//! let extent = Extent3::from_min_and_shape(PointN([0; 3]), PointN([16; 3]));
//! let mut index_map = Array3::fill(extent, 0u8);
//! *index_map.get_mut(&PointN([0, 0, 1])) = 1;
//!
//! let palette = vec![BigData([1; 9001]), BigData([2; 9001])];
//! let lookup = |i: &u8| &palette[*i as usize];
//! let big_data_map = TransformMap::new(&index_map, &lookup);
//!
//! assert_eq!(big_data_map.get_ref(&PointN([0, 0, 0])).0.as_ptr(), palette[0].0.as_ptr());
//! assert_eq!(big_data_map.get_ref(&PointN([0, 0, 1])).0.as_ptr(), palette[1].0.as_ptr());
//! ```
//!
//! `TransformMap` also gives us an efficient way of applying transforms to array data during a
//! copy:
//!
//! ```
//! # use building_blocks_core::prelude::*;
//! # use building_blocks_storage::prelude::*;
//! # let extent = Extent3::from_min_and_shape(PointN([0; 3]), PointN([16; 3]));
//! let src = Array3::fill(extent, 0);
//! let mut dst = ChunkMap3::new(PointN([4; 3]), 0, (), FastLz4 { level: 10 });
//! let tfm = TransformMap::new(&src, &|value: i32| value + 1);
//! copy_extent(&extent, &tfm, &mut dst);
//! ```

use crate::{
    access::{GetUnchecked, GetUncheckedRef},
    array::ArrayCopySrc,
    chunk_map::{AmbientExtent, ArrayChunkCopySrc, ArrayChunkCopySrcIter, ChunkCopySrc},
    ArrayExtent, ArrayN, ChunkMapReader, ForEachRef, Get, GetRef, ReadExtent,
};

use building_blocks_core::prelude::*;

use core::hash::Hash;
use core::iter::{once, Once};

/// A lattice map that delegates look-ups to a different lattice map, then transforms the result
/// using some `Fn(T) -> S`.
pub struct TransformMap<'a, M, F> {
    delegate: &'a M,
    transform: &'a F,
}

impl<'a, M, F> Clone for TransformMap<'a, M, F> {
    fn clone(&self) -> Self {
        Self {
            delegate: self.delegate,
            transform: self.transform,
        }
    }
}
impl<'a, M, F> Copy for TransformMap<'a, M, F> {}

impl<'a, M, F> TransformMap<'a, M, F> {
    pub fn new(delegate: &'a M, transform: &'a F) -> Self {
        Self {
            delegate,
            transform,
        }
    }
}

impl<'a, M, F, T, S, Coord> Get<Coord> for TransformMap<'a, M, F>
where
    F: Fn(T) -> S,
    M: Get<Coord, Data = T>,
{
    type Data = S;

    fn get(&self, c: Coord) -> S {
        (self.transform)(self.delegate.get(c))
    }
}

impl<'a, M, F, T, S, Coord> GetRef<Coord> for TransformMap<'a, M, F>
where
    T: 'a,
    S: 'a,
    F: Fn(&'a T) -> &'a S,
    M: GetRef<Coord, Data = T>,
{
    type Data = S;

    fn get_ref(&self, c: Coord) -> &S {
        (self.transform)(self.delegate.get_ref(c))
    }
}

impl<'a, M, F, T, S, Coord> GetUnchecked<Coord> for TransformMap<'a, M, F>
where
    F: Fn(T) -> S,
    M: GetUnchecked<Coord, Data = T>,
{
    type Data = S;

    unsafe fn get_unchecked(&self, c: Coord) -> S {
        (self.transform)(self.delegate.get_unchecked(c))
    }
}

impl<'a, M, F, T, S, Coord> GetUncheckedRef<Coord> for TransformMap<'a, M, F>
where
    T: 'a,
    S: 'a,
    F: Fn(&'a T) -> &'a S,
    M: GetUncheckedRef<Coord, Data = T>,
{
    type Data = S;

    unsafe fn get_unchecked_ref(&self, c: Coord) -> &S {
        (self.transform)(self.delegate.get_unchecked_ref(c))
    }
}

impl<'a, M, F, N, T, S, Coord> ForEachRef<N, Coord> for TransformMap<'a, M, F>
where
    T: 'a,
    S: 'a,
    F: for<'r> Fn(&'r T) -> &'a S,
    M: ForEachRef<N, Coord, Data = T>,
{
    type Data = S;

    fn for_each_ref(&self, extent: &ExtentN<N>, mut f: impl FnMut(Coord, &Self::Data)) {
        self.delegate
            .for_each_ref(extent, |c, t| f(c, (self.transform)(t)))
    }
}

impl<'a, N, M, F> ArrayExtent<N> for TransformMap<'a, M, F>
where
    M: ArrayExtent<N>,
{
    fn extent(&self) -> &ExtentN<N> {
        self.delegate.extent()
    }
}

// TODO: try to make a generic ReadExtent impl, it's hard because we need a way to define the src
// types as a function of the delegate src types (kinda hints at a monad or HKT)

impl<'a, F, S, N, T> ReadExtent<'a, N> for TransformMap<'a, ArrayN<N, S>, F>
where
    Self: ArrayExtent<N>,
    F: 'a + Fn(S) -> T,
    PointN<N>: IntegerPoint,
{
    type Src = ArrayCopySrc<Self>;
    type SrcIter = Once<(ExtentN<N>, Self::Src)>;

    fn read_extent(&'a self, extent: &ExtentN<N>) -> Self::SrcIter {
        let in_bounds_extent = self.extent().intersection(extent);

        once((in_bounds_extent, ArrayCopySrc(*self)))
    }
}

impl<'a, F, S, N, T, M> ReadExtent<'a, N> for TransformMap<'a, ChunkMapReader<'a, N, S, M>, F>
where
    ChunkMapReader<'a, N, S, M>: ReadExtent<
        'a,
        N,
        Src = ArrayChunkCopySrc<'a, N, S>,
        SrcIter = ArrayChunkCopySrcIter<'a, N, S>,
    >,
    F: 'a + Fn(S) -> T,
    S: Copy,
    T: 'a,
    M: Clone,
    PointN<N>: Point + Eq + Hash,
    ExtentN<N>: IntegerExtent<N>,
{
    type Src = TransformChunkCopySrc<'a, F, S, N, T>;
    type SrcIter = TransformChunkCopySrcIter<'a, F, S, N, T>;

    fn read_extent(&'a self, extent: &ExtentN<N>) -> Self::SrcIter {
        TransformChunkCopySrcIter {
            chunk_iter: self.delegate.read_extent(extent),
            transform: &self.transform,
        }
    }
}

pub type TransformChunkCopySrc<'a, F, S, N, T> =
    ChunkCopySrc<TransformMap<'a, ArrayN<N, S>, F>, N, T>;

pub struct TransformChunkCopySrcIter<'a, F, S, N, T>
where
    F: Fn(S) -> T,
{
    chunk_iter: ArrayChunkCopySrcIter<'a, N, S>,
    transform: &'a F,
}

impl<'a, F, S, N, T> Iterator for TransformChunkCopySrcIter<'a, F, S, N, T>
where
    F: 'a + Fn(S) -> T,
{
    type Item = (ExtentN<N>, TransformChunkCopySrc<'a, F, S, N, T>);

    fn next(&mut self) -> Option<Self::Item> {
        self.chunk_iter.next().map(|(extent, chunk_src)| {
            (
                extent,
                chunk_src
                    .map_left(|array_src| {
                        ArrayCopySrc(TransformMap::new(array_src.0, self.transform))
                    })
                    .map_right(|ambient| AmbientExtent::new((self.transform)(ambient.value))),
            )
        })
    }
}

// ████████╗███████╗███████╗████████╗
// ╚══██╔══╝██╔════╝██╔════╝╚══██╔══╝
//    ██║   █████╗  ███████╗   ██║
//    ██║   ██╔══╝  ╚════██║   ██║
//    ██║   ███████╗███████║   ██║
//    ╚═╝   ╚══════╝╚══════╝   ╚═╝

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prelude::*;

    #[test]
    fn transform_accessors() {
        let extent = Extent3::from_min_and_shape(PointN([0; 3]), PointN([16; 3]));
        let inner_map: Array3<usize> = Array3::fill(extent, 0usize);

        let palette = vec![1, 2, 3];
        let f = |i: &usize| &palette[*i];
        let outer_map = TransformMap::new(&inner_map, &f);

        assert_eq!(outer_map.get_ref(&PointN([0; 3])), &1);

        outer_map.for_each_ref(&extent, |_s: Stride, value| {
            assert_eq!(value, &1);
        });
        outer_map.for_each_ref(&extent, |_p: Point3i, value| {
            assert_eq!(value, &1);
        });
        outer_map.for_each_ref(&extent, |_ps: (Point3i, Stride), value| {
            assert_eq!(value, &1);
        });

        let f = |i: usize| palette[i];
        let outer_map = TransformMap::new(&inner_map, &f);
        assert_eq!(outer_map.get(&PointN([0; 3])), 1);
    }

    #[test]
    fn copy_from_transformed_array() {
        let extent = Extent3::from_min_and_shape(PointN([0; 3]), PointN([16; 3]));
        let src = Array3::fill(extent, 0);
        let mut dst = ChunkMap3::new(PointN([4; 3]), 0, (), FastLz4 { level: 10 });
        let tfm = TransformMap::new(&src, &|value: i32| value + 1);
        copy_extent(&extent, &tfm, &mut dst);
    }

    #[test]
    fn copy_from_transformed_chunk_map_reader() {
        let src_extent = Extent3::from_min_and_shape(PointN([0; 3]), PointN([16; 3]));
        let src_array = Array3::fill(src_extent, 1);
        let mut src = ChunkMap3::new(PointN([4; 3]), 0, (), FastLz4 { level: 10 });
        copy_extent(&src_extent, &src_array, &mut src);

        let local_cache = LocalChunkCache::new();
        let src_reader = ChunkMapReader3::new(&src, &local_cache);
        let tfm = TransformMap::new(&src_reader, &|value: i32| value + 1);

        let dst_extent = Extent3::from_min_and_shape(PointN([-16; 3]), PointN([32; 3]));
        let mut dst = ChunkMap3::new(PointN([2; 3]), 0, (), FastLz4 { level: 10 });
        copy_extent(&dst_extent, &tfm, &mut dst);
    }
}
