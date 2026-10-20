//! A landblock's indoor cells exactly as the game data places them: which
//! environment piece each is built from and where that piece stands. A
//! plugin that draws dungeons from its own tiles, one per piece, places each
//! tile with this.

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use ac_dat::DatError;
use ac_dat::objects::{EnvCell, EnvCellFlags, LandBlockInfo};
use ac_physics::content::DatSource;
use glam::{Quat, Vec3};

/// How many landblocks' indoor cells are kept; the landblock read longest
/// ago goes first once it is full.
pub const MAXIMUM_CACHED_INDOOR_LANDBLOCKS: usize = 64;

/// One indoor cell as the data stores it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct IndoorCell {
    /// The full cell id, landblock in the high half.
    pub cell_id: u32,
    /// The environment piece (low sixteen bits of its file id); zero for a
    /// cell that names none.
    pub environment_id: u32,
    /// Which of the piece's structures the cell uses.
    pub cell_structure: u32,
    /// Where the piece's origin sits, landblock-local metres.
    pub origin: Vec3,
    /// How the piece is turned, as stored.
    pub orientation: Quat,
    /// The cell sees the landscape, as a building's rooms do.
    pub sees_outside: bool,
}

/// The indoor cells of the landblocks asked about, read once each.
#[derive(Debug, Default)]
pub struct IndoorCellCache {
    kept: HashMap<u32, Arc<[IndoorCell]>>,
    order: VecDeque<u32>,
}

impl IndoorCellCache {
    /// Nothing kept yet.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The indoor cells of the landblock of `landblock_id` (any cell id in
    /// it will do), in cell order; empty for a landblock with none or not in
    /// the data. The same list comes back until the cache drops it.
    ///
    /// # Errors
    /// A dat read or decode failure; nothing is kept then.
    pub fn capture<D: DatSource>(
        &mut self,
        dats: &D,
        landblock_id: u32,
    ) -> Result<Arc<[IndoorCell]>, DatError> {
        let landblock = landblock_id & 0xFFFF_0000;
        if let Some(kept) = self.kept.get(&landblock) {
            return Ok(Arc::clone(kept));
        }
        let cells: Arc<[IndoorCell]> = read_indoor_cells(dats, landblock)?.into();
        if self.kept.len() >= MAXIMUM_CACHED_INDOOR_LANDBLOCKS
            && let Some(oldest) = self.order.pop_front()
        {
            self.kept.remove(&oldest);
        }
        self.kept.insert(landblock, Arc::clone(&cells));
        self.order.push_back(landblock);
        Ok(cells)
    }
}

/// The landblock's cell count from its information file, then each cell the
/// count covers, as stored. A cell the files do not have is left out.
///
/// # Errors
/// A dat read or decode failure.
pub fn read_indoor_cells<D: DatSource>(
    dats: &D,
    landblock: u32,
) -> Result<Vec<IndoorCell>, DatError> {
    let landblock = landblock & 0xFFFF_0000;
    let mut cells = Vec::new();
    let Some(info) = dats.object::<LandBlockInfo>(landblock | 0xFFFE)? else {
        return Ok(cells);
    };
    let first_cell = landblock | 0x0100;
    for offset in 0..info.num_cells {
        let cell_id = first_cell + offset;
        let Some(cell) = dats.object::<EnvCell>(cell_id)? else {
            continue;
        };
        let o = cell.position.orientation;
        let p = cell.position.origin;
        cells.push(IndoorCell {
            cell_id,
            environment_id: u32::from(cell.environment_id),
            cell_structure: u32::from(cell.cell_structure),
            origin: Vec3::new(p.x, p.y, p.z),
            orientation: Quat::from_xyzw(o.x, o.y, o.z, o.w),
            sees_outside: cell.flags.contains(EnvCellFlags::SEEN_OUTSIDE),
        });
    }
    Ok(cells)
}

#[cfg(test)]
mod tests {
    //! Ported from OpenAC `tests/AcDream.Runtime.Tests/Maps/IndoorCellCaptureTests.cs`
    //! over its `TwoRoomLandblock.cs` fixture (shared with the floorplan
    //! tests). The read-only list test has no counterpart: the shared list
    //! is an `Arc<[_]>`, which cannot be changed through.

    use super::*;
    use crate::floorplan::tests::{
        CELL_SEEING_OUTSIDE, EMPTY_LANDBLOCK, TURNED_ROOM, TWO_ROOMS, UPSTAIRS_ROOM, two_rooms,
    };

    #[test]
    fn every_listed_cell_comes_back_with_its_piece_and_placement() {
        let dats = two_rooms();
        let mut cache = IndoorCellCache::new();
        let cells = cache.capture(&dats, TURNED_ROOM).unwrap();

        let ids: Vec<u32> = cells.iter().map(|c| c.cell_id).collect();
        assert_eq!(ids, [TURNED_ROOM, UPSTAIRS_ROOM, CELL_SEEING_OUTSIDE]);

        let turned = cells[0];
        assert_eq!(turned.environment_id, 0x0123);
        assert_eq!(turned.cell_structure, 1);
        assert_eq!(turned.origin, Vec3::new(20.0, 30.0, 0.5));
        let quarter = Quat::from_axis_angle(Vec3::Z, std::f32::consts::FRAC_PI_2);
        assert!(turned.orientation.abs_diff_eq(quarter, 1e-6));
        assert!(!turned.sees_outside);

        let upstairs = cells[1];
        assert_eq!(upstairs.cell_structure, 2);
        assert_eq!(upstairs.origin, Vec3::new(40.0, 30.0, 7.2));
        assert_eq!(upstairs.orientation, Quat::IDENTITY);

        let outside = cells[2];
        assert_eq!(outside.environment_id, 0);
        assert!(outside.sees_outside);

        let reads = dats.reads();
        let again = cache.capture(&dats, TWO_ROOMS).unwrap();
        assert!(Arc::ptr_eq(&cells, &again));
        assert_eq!(dats.reads(), reads);
    }

    #[test]
    fn a_landblock_without_cells_or_without_files_has_none() {
        let dats = two_rooms();
        let mut cache = IndoorCellCache::new();
        assert!(cache.capture(&dats, EMPTY_LANDBLOCK).unwrap().is_empty());
        assert!(cache.capture(&dats, 0x7F7F_0000).unwrap().is_empty());
    }

    #[test]
    fn the_landblock_read_longest_ago_is_dropped_once_the_cache_is_full() {
        let dats = two_rooms();
        let mut cache = IndoorCellCache::new();
        let first = cache.capture(&dats, TWO_ROOMS).unwrap();

        for index in 1..MAXIMUM_CACHED_INDOOR_LANDBLOCKS {
            let other = 0x7000_0000 + (u32::try_from(index).unwrap() << 16);
            let _ = cache.capture(&dats, other).unwrap();
        }
        assert!(Arc::ptr_eq(
            &first,
            &cache.capture(&dats, TWO_ROOMS).unwrap()
        ));

        let _ = cache.capture(&dats, 0x6F00_0000).unwrap();
        let again = cache.capture(&dats, TWO_ROOMS).unwrap();
        assert!(!Arc::ptr_eq(&first, &again));
        assert_eq!(*first, *again);
    }
}
