//! The `Octree` type is a memory-efficient set of points.
//!
//! The typical workflow for using an `Octree` is to construct it from an `Array3`, then insert it
//! into an `OctreeDBVT` in order to perform spatial queries like raycasting.

use building_blocks_core::prelude::*;
use building_blocks_storage::{prelude::*, IsEmpty};

use fnv::FnvHashMap;

/// A sparse set of voxel coordinates (3D integer points). Supports spatial queries.
///
/// The octree is a cube shape and the edge lengths can only be a power of 2, at most 64. When an
/// entire octant is full, it will be stored in a collapsed representation, so the leaves of the
/// tree can be differently sized octants.
pub struct Octree {
    extent: Extent3i,
    root_level: u8,
    root_exists: bool,
    // Save memory by using 2-byte location codes as hash map keys instead of 64-bit node pointers.
    // The total memory usage can be approximated as 3 bytes per node, assuming a hashbrown table.
    nodes: FnvHashMap<LocationCode, ChildBitMask>,
}

impl Octree {
    /// Constructs an `Octree` which contains all of the points which are not empty (as defined by
    /// the `IsEmpty` trait). `array` must be cube-shaped with edge length being a power of 2.
    /// `power` must be the exponent of the edge length, and `0 < power <= 6`.
    pub fn from_array<T: IsEmpty>(power: u8, array: &Array3<T>) -> Self {
        // Constrained by 16-bit location code.
        assert!(power > 0 && power <= 6);
        let root_level = power - 1;
        let edge_len = 1 << power;
        assert_eq!(PointN([edge_len; 3]), array.extent().shape);

        // These are the corners of the root octant, in local coordinates.
        let corner_offsets: Vec<_> = Point3i::corner_offsets()
            .into_iter()
            .map(|p| p * edge_len)
            .collect();
        // Convert into strides for indexing efficiency.
        let mut corner_strides = [Stride(0); 8];
        array.strides_from_points(&corner_offsets, &mut corner_strides);

        let mut nodes = FnvHashMap::default();
        let root_minimum = Stride(0);
        let root_location = LocationCode(1);
        let root_exists = Self::partition_array(
            root_location,
            root_minimum,
            edge_len,
            &corner_strides,
            array,
            &mut nodes,
        );

        Octree {
            root_level,
            root_exists,
            extent: *array.extent(),
            nodes,
        }
    }

    fn partition_array<T: IsEmpty>(
        location: LocationCode,
        minimum: Stride,
        edge_len: i32,
        corner_strides: &[Stride],
        array: &Array3<T>,
        nodes: &mut FnvHashMap<LocationCode, ChildBitMask>,
    ) -> bool {
        // Base case where the octant is a single voxel.
        if edge_len == 1 {
            return !array.get_ref(minimum).is_empty();
        }

        let mut octant_corner_strides = [Stride(0); 8];
        for (child_corner, parent_corner) in
            octant_corner_strides.iter_mut().zip(corner_strides.iter())
        {
            *child_corner = Stride(parent_corner.0 >> 1);
        }

        let half_edge_len = edge_len >> 1;
        let mut child_bitmask = 0;
        let extended_location = location.extend();
        for (octant, offset) in octant_corner_strides.iter().enumerate() {
            let octant_min = minimum + *offset;
            let octant_location = extended_location.with_lowest_octant(octant as u16);
            let child_exists = Self::partition_array(
                octant_location,
                octant_min,
                half_edge_len,
                &octant_corner_strides,
                array,
                nodes,
            );
            child_bitmask |= (child_exists as u8) << octant;
        }

        let is_leaf = child_bitmask == 0xff;
        let exists = child_bitmask != 0;

        if exists && !is_leaf {
            nodes.insert(location, child_bitmask);
        }

        exists
    }

    pub fn edge_length(&self) -> i32 {
        1 << (self.root_level + 1)
    }

    /// The entire octant spanned by the octree.
    pub fn octant(&self) -> Octant {
        Octant {
            minimum: self.extent.minimum,
            edge_length: self.edge_length(),
        }
    }

    /// The extent spanned by the octree.
    pub fn extent(&self) -> &Extent3i {
        &self.extent
    }

    /// Returns `true` iff the octree contains zero points.
    pub fn is_empty(&self) -> bool {
        !self.root_exists
    }

    /// Visit every non-empty octant of the octree.
    pub fn visit(&self, visitor: &mut impl OctreeVisitor) -> VisitStatus {
        if !self.root_exists {
            return VisitStatus::Continue;
        }

        let minimum = self.extent.minimum;
        let edge_len = self.edge_length();
        let corner_offsets: Vec<_> = Point3i::corner_offsets()
            .into_iter()
            .map(|p| p * edge_len)
            .collect();

        self._visit(LocationCode(1), minimum, edge_len, &corner_offsets, visitor)
    }

    fn _visit(
        &self,
        location: LocationCode,
        minimum: Point3i,
        edge_length: i32,
        corner_offsets: &[Point3i],
        visitor: &mut impl OctreeVisitor,
    ) -> VisitStatus {
        // Precondition: location exists.

        // Base case where the octant is a single leaf voxel.
        if edge_length == 1 {
            return visitor.visit_octant(
                Octant {
                    minimum,
                    edge_length,
                },
                true,
            );
        }

        // Continue traversal of this branch.

        let child_bitmask = if let Some(child_bitmask) = self.nodes.get(&location) {
            child_bitmask
        } else {
            // Since we know that location exists, but it's not in the nodes map, this means that we
            // can assume the entire octant is full. This is an implicit leaf node.
            return visitor.visit_octant(
                Octant {
                    minimum,
                    edge_length,
                },
                true,
            );
        };

        // Definitely not at a leaf node.
        let status = visitor.visit_octant(
            Octant {
                minimum,
                edge_length,
            },
            false,
        );
        if status != VisitStatus::Continue {
            return status;
        }

        let mut octant_corner_offsets = [PointN([0; 3]); 8];
        for (child_corner, parent_corner) in
            octant_corner_offsets.iter_mut().zip(corner_offsets.iter())
        {
            *child_corner = parent_corner.right_shift(1);
        }

        let half_edge_length = edge_length >> 1;
        let extended_location = location.extend();
        for (octant, offset) in octant_corner_offsets.iter().enumerate() {
            if (child_bitmask & (1 << octant)) == 0 {
                // This child does not exist.
                continue;
            }

            let octant_min = minimum + *offset;
            let octant_location = extended_location.with_lowest_octant(octant as u16);
            if self._visit(
                octant_location,
                octant_min,
                half_edge_length,
                &octant_corner_offsets,
                visitor,
            ) == VisitStatus::ExitEarly
            {
                return VisitStatus::ExitEarly;
            }
        }

        // Continue with the rest of the tree.
        VisitStatus::Continue
    }
}

type ChildBitMask = u8;

/// Uniquely identifies a location in a given octree.
///
/// Supports an octree with at most 6 levels.
/// ```text
/// level N:
///   loc = 0b1
/// level N-1:
///   loc = 0b1000, 0b1001, 0b1010, 0b1011, 0b1100, 0b1101, 0b1110, 0b1111
/// level N-2:
///   loc = 0b1000000, ...
/// ...
/// level N-5:
///   loc = 0b1000000000000000, ...
/// ```
#[derive(Clone, Copy, Debug, Hash, Eq, PartialEq)]
struct LocationCode(u16);

impl LocationCode {
    pub fn extend(self) -> Self {
        LocationCode(self.0 << 3)
    }

    pub fn with_lowest_octant(self, octant: u16) -> Self {
        LocationCode(self.0 | octant)
    }
}

/// A cube-shaped extent which is an octant at some level of an octree. As a leaf node, it
/// represents a totally full set of points.
#[derive(Clone, Copy)]
pub struct Octant {
    pub minimum: Point3i,
    pub edge_length: i32,
}

pub trait OctreeVisitor {
    /// Visit any octant that contains points in the octree.
    fn visit_octant(&mut self, octant: Octant, is_leaf: bool) -> VisitStatus;
}

#[derive(Eq, PartialEq)]
pub enum VisitStatus {
    /// Continue traversing this branch.
    Continue,
    /// Stop traversing this branch.
    Stop,
    /// Stop traversing the entire tree. No further nodes will be visited.
    ExitEarly,
}

#[cfg(feature = "ncollide")]
mod ncollide_support {
    use super::*;

    use ncollide3d::bounding_volume::AABB;

    impl Octant {
        pub fn aabb(&self) -> AABB<f32> {
            let aabb_min = self.minimum;
            let aabb_max = self.minimum + PointN([self.edge_length; 3]);

            AABB::new(aabb_min.into(), aabb_max.into())
        }
    }
}
