use crate::{Array, ArrayN, Local, Stride};

use building_blocks_core::prelude::*;

/// Map-local coordinates, wrapping a `Point3i`.
pub type Local3i = Local<[i32; 3]>;

pub type Array3<T> = ArrayN<[i32; 3], T>;

impl<T> Array<[i32; 3]> for Array3<T> {
    #[inline]
    fn stride_from_point(s: &Point3i, p: &Point3i) -> Stride {
        Stride((p.z() * s.y() * s.x() + p.y() * s.x() + p.x()) as usize)
    }

    fn for_each_point_and_stride(
        array_extent: &Extent3i,
        extent: &Extent3i,
        mut f: impl FnMut(Point3i, Stride),
    ) {
        // Translate to local coordinates.
        let global_extent = extent.intersection(array_extent);
        let global_lub = global_extent.least_upper_bound();
        let local_extent = global_extent - array_extent.minimum;

        let mut s = Array3ForEachState::new(&array_extent.shape, &Local(local_extent.minimum));
        s.start_z();
        for z in global_extent.minimum.z()..global_lub.z() {
            s.start_y();
            for y in global_extent.minimum.y()..global_lub.y() {
                s.start_x();
                for x in global_extent.minimum.x()..global_lub.x() {
                    f(PointN([x, y, z]), s.stride());
                    s.incr_x();
                }
                s.incr_y();
            }
            s.incr_z();
        }
    }

    fn for_each_stride_parallel(
        iter_extent: &Extent3i,
        array1_extent: &Extent3i,
        array2_extent: &Extent3i,
        mut f: impl FnMut(Stride, Stride),
    ) {
        // Translate to local coordinates.
        let min1 = iter_extent.minimum - array1_extent.minimum;
        let min2 = iter_extent.minimum - array2_extent.minimum;

        let mut s1 = Array3ForEachState::new(&array1_extent.shape, &Local(min1));
        let mut s2 = Array3ForEachState::new(&array2_extent.shape, &Local(min2));

        s1.start_z();
        s2.start_z();
        for _z in 0..iter_extent.shape.z() {
            s1.start_y();
            s2.start_y();
            for _y in 0..iter_extent.shape.y() {
                s1.start_x();
                s2.start_x();
                for _x in 0..iter_extent.shape.x() {
                    f(s1.stride(), s2.stride());

                    s1.incr_x();
                    s2.incr_x();
                }
                s1.incr_y();
                s2.incr_y();
            }
            s1.incr_z();
            s2.incr_z();
        }
    }
}

struct Array3ForEachState {
    x_stride: usize,
    y_stride: usize,
    z_stride: usize,
    x_start: usize,
    y_start: usize,
    z_start: usize,
    x_i: usize,
    y_i: usize,
    z_i: usize,
}

impl Array3ForEachState {
    fn new(array_shape: &Point3i, iter_min: &Local3i) -> Self {
        let x_stride = 1usize;
        let y_stride = array_shape.x() as usize;
        let z_stride = (array_shape.y() * array_shape.x()) as usize;
        let x_start = x_stride * iter_min.0.x() as usize;
        let y_start = y_stride * iter_min.0.y() as usize;
        let z_start = z_stride * iter_min.0.z() as usize;

        Self {
            x_stride,
            y_stride,
            z_stride,
            x_start,
            y_start,
            z_start,
            x_i: 0,
            y_i: 0,
            z_i: 0,
        }
    }

    fn stride(&self) -> Stride {
        Stride(self.x_i)
    }

    fn start_z(&mut self) {
        self.z_i = self.z_start;
    }
    fn start_y(&mut self) {
        self.y_i = self.z_i + self.y_start;
    }
    fn start_x(&mut self) {
        self.x_i = self.y_i + self.x_start;
    }

    fn incr_x(&mut self) {
        self.x_i += self.x_stride;
    }
    fn incr_y(&mut self) {
        self.y_i += self.y_stride;
    }
    fn incr_z(&mut self) {
        self.z_i += self.z_stride;
    }
}
