//! The plugin API's layout fingerprint: the shape of everything that
//! crosses the boundary, written into every plugin, so a plugin built
//! against other record layouts is refused at load instead of failing on
//! the first reply that decodes wrongly.
//!
//! The rule: **a record never changes shape silently.** Adding, removing,
//! renaming, reordering or retyping a field, a variant or a tag of any
//! record, reply or event changes its shape, so the fingerprint changes,
//! and the plugins that use it must be rebuilt against the new SDK.
//!
//! The SDK writes, into the plugin's `.wasm` ([`LAYOUT_SECTION`]), the
//! shapes it was built with: one for what every plugin decodes (the start
//! record, the events, the callbacks and their answers: [`CORE_SHAPE`]),
//! and one per import of its table ([`Import::shape`]: the name, the
//! request and the reply). The client and `plugincheck` refuse a module
//! whose core shape differs from theirs, or that imports a function whose
//! shape differs, or that has no section. A plugin is not refused for an
//! import it does not use, so a new import alone needs no rebuild.
//!
//! Nothing here is written by hand: each type's [`Wire::SHAPE`] is computed
//! by the compiler from the definition that also writes its encoding
//! ([`crate::wire_record!`] and its siblings), each import's from its table
//! entry, and the section from those.

use crate::callback::{Callback, CallbackAnswer};
use crate::imports::Import;
use crate::types::{Event, StartInfo};
use crate::wire::{Shape, Wire};

/// The custom section the SDK writes into every plugin's `.wasm`:
/// [`CORE_SHAPE`], then for each import of the SDK's table the
/// [`import_key`] of its name and its [`Import::shape`], every number a
/// little-endian `u64`.
pub const LAYOUT_SECTION: &str = "openac.layout";

/// The shape of what every plugin decodes or answers whatever it imports:
/// the start record, the events, the callbacks and their answers.
pub const CORE_SHAPE: u64 = Shape::new("openac plugin api core")
    .of(StartInfo::SHAPE)
    .of(Event::SHAPE)
    .of(Callback::SHAPE)
    .of(CallbackAnswer::SHAPE)
    .finish();

/// The whole layout in one number, for people (reports, the published
/// value the ABI's tests pin): [`CORE_SHAPE`] and every import's shape, in
/// any order (the table's order does not matter to a plugin).
pub const LAYOUT_FINGERPRINT: u64 = fingerprint();

/// The length of [`LAYOUT_SECTION_BYTES`].
pub const LAYOUT_SECTION_LEN: usize = 8 + 16 * Import::ALL.len();

/// The contents of [`LAYOUT_SECTION`] for this build of the ABI.
pub const LAYOUT_SECTION_BYTES: [u8; LAYOUT_SECTION_LEN] = section();

/// The key an import's name has in [`LAYOUT_SECTION`].
#[must_use]
pub const fn import_key(name: &str) -> u64 {
    Shape::new("import name").name(name).finish()
}

const fn fingerprint() -> u64 {
    // Each import is mixed on its own and the results summed, so the sum
    // does not depend on the table's order.
    let mut imports: u64 = 0;
    let mut at = 0;
    while at < Import::ALL.len() {
        imports = imports.wrapping_add(mix(Import::ALL[at].shape()));
        at += 1;
    }
    Shape::new("openac plugin api layout")
        .of(CORE_SHAPE)
        .of(imports)
        .finish()
}

/// Spreads every bit of `value` over the whole word (the SplitMix64
/// finaliser), so a sum of mixed values keeps each value's bits.
const fn mix(value: u64) -> u64 {
    let mut x = value;
    x ^= x >> 30;
    x = x.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x ^= x >> 27;
    x = x.wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^ (x >> 31)
}

const fn put(out: &mut [u8; LAYOUT_SECTION_LEN], at: usize, value: u64) {
    let bytes = value.to_le_bytes();
    let mut i = 0;
    while i < 8 {
        out[at + i] = bytes[i];
        i += 1;
    }
}

const fn section() -> [u8; LAYOUT_SECTION_LEN] {
    let mut out = [0u8; LAYOUT_SECTION_LEN];
    put(&mut out, 0, CORE_SHAPE);
    let mut at = 0;
    while at < Import::ALL.len() {
        let import = Import::ALL[at];
        put(&mut out, 8 + 16 * at, import_key(import.name()));
        put(&mut out, 16 + 16 * at, import.shape());
        at += 1;
    }
    out
}

/// A plugin's [`LAYOUT_SECTION`], read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginLayout {
    /// The plugin's [`CORE_SHAPE`].
    pub core: u64,
    /// Its imports' shapes by [`import_key`], sorted by key.
    imports: Vec<(u64, u64)>,
}

impl PluginLayout {
    /// Reads a section: a core shape and whole `(key, shape)` pairs.
    #[must_use]
    pub fn parse(bytes: &[u8]) -> Option<Self> {
        let (core, rest) = bytes.split_first_chunk::<8>()?;
        if rest.len() % 16 != 0 {
            return None;
        }
        let word = |chunk: &[u8]| u64::from_le_bytes(chunk.try_into().unwrap_or([0; 8]));
        let mut imports: Vec<(u64, u64)> = rest
            .chunks_exact(16)
            .map(|pair| (word(&pair[..8]), word(&pair[8..])))
            .collect();
        imports.sort_unstable();
        Some(Self {
            core: u64::from_le_bytes(*core),
            imports,
        })
    }

    /// The shape the plugin was built with for the import named `name`;
    /// none when its SDK did not have it.
    #[must_use]
    pub fn import_shape(&self, name: &str) -> Option<u64> {
        let key = import_key(name);
        self.imports
            .binary_search_by_key(&key, |&(k, _)| k)
            .ok()
            .map(|at| self.imports[at].1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{wire_enum, wire_record, wire_union};

    /// The fingerprint this ABI was last published with. A failure here
    /// means a record, reply, event or import changed: allowed while API
    /// 1.0 is unfrozen, but never silently. Update this value in the same
    /// commit; when an existing record changed shape (not only a new
    /// import), say in the commit message which plugins must be rebuilt.
    const PUBLISHED: u64 = 0x5c7a_6263_52ee_b019;

    #[test]
    fn the_fingerprint_is_the_published_one() {
        assert_eq!(
            LAYOUT_FINGERPRINT, PUBLISHED,
            "the plugin API layout changed (fingerprint {LAYOUT_FINGERPRINT:016x}, published \
             {PUBLISHED:016x}): update PUBLISHED and rebuild the plugins it affects"
        );
    }

    mod before {
        crate::wire_record! {
            /// A corpse as an older SDK knew it.
            pub struct Loot {
                /// Its id.
                pub id: u32,
                /// Its name.
                pub name: String,
            }
        }
    }

    mod after {
        crate::wire_record! {
            /// The same corpse with a field added at the end.
            pub struct Loot {
                /// Its id.
                pub id: u32,
                /// Its name.
                pub name: String,
                /// Whether its appraisal was answered.
                pub answered: bool,
            }
        }
    }

    mod renamed {
        crate::wire_record! {
            /// The same fields, one renamed.
            pub struct Loot {
                /// Its id.
                pub object_id: u32,
                /// Its name.
                pub name: String,
            }
        }
    }

    mod reordered {
        crate::wire_record! {
            /// The same fields in another order.
            pub struct Loot {
                /// Its name.
                pub name: String,
                /// Its id.
                pub id: u32,
            }
        }
    }

    mod retyped {
        crate::wire_record! {
            /// The same fields, one of another type.
            pub struct Loot {
                /// Its id.
                pub id: u64,
                /// Its name.
                pub name: String,
            }
        }
    }

    wire_record! {
        /// A reply holding the older record, as `loot_corpses` does.
        pub struct OldReply {
            /// The corpses.
            pub corpses: Vec<Option<before::Loot>>,
        }
    }

    wire_record! {
        /// The same reply holding the newer record.
        pub struct OldReplyNewLoot {
            /// The corpses.
            pub corpses: Vec<Option<after::Loot>>,
        }
    }

    #[test]
    fn any_change_to_a_record_changes_its_shape() {
        let shapes = [
            before::Loot::SHAPE,
            after::Loot::SHAPE,
            renamed::Loot::SHAPE,
            reordered::Loot::SHAPE,
            retyped::Loot::SHAPE,
        ];
        for (i, a) in shapes.iter().enumerate() {
            for b in &shapes[i + 1..] {
                assert_ne!(a, b);
            }
        }
    }

    #[test]
    fn a_change_inside_a_record_reaches_everything_that_holds_it() {
        assert_ne!(OldReply::SHAPE, OldReplyNewLoot::SHAPE);
        assert_ne!(Vec::<before::Loot>::SHAPE, Vec::<after::Loot>::SHAPE);
        assert_ne!(Vec::<u32>::SHAPE, Option::<u32>::SHAPE);
        assert_ne!(<(u32, String)>::SHAPE, <(String, u32)>::SHAPE);
        assert_ne!(
            Result::<before::Loot, String>::SHAPE,
            Result::<after::Loot, String>::SHAPE
        );
    }

    wire_enum! {
        /// An enum.
        pub enum Two: "two" {
            /// First.
            A = 0,
            /// Second.
            B = 1,
        }
    }

    wire_enum! {
        /// The same enum with a variant added.
        pub enum Three: "three" {
            /// First.
            A = 0,
            /// Second.
            B = 1,
            /// Third.
            C = 2,
        }
    }

    wire_enum! {
        /// The same variants, one tag moved.
        pub enum Moved: "moved" {
            /// First.
            A = 0,
            /// Second.
            B = 2,
        }
    }

    wire_union! {
        /// A union.
        pub enum Small: "small" {
            /// Nothing.
            0 => Empty,
            /// A number.
            1 => Number(u32),
        }
    }

    wire_union! {
        /// The same union, its payload retyped.
        pub enum SmallRetyped: "small" {
            /// Nothing.
            0 => Empty,
            /// A number.
            1 => Number(i32),
        }
    }

    #[test]
    fn variants_tags_and_payloads_are_part_of_the_shape() {
        assert_ne!(Two::SHAPE, Three::SHAPE);
        assert_ne!(Two::SHAPE, Moved::SHAPE);
        assert_ne!(Small::SHAPE, SmallRetyped::SHAPE);
    }

    #[test]
    fn every_import_has_its_own_shape_and_key() {
        let mut shapes: Vec<u64> = Import::ALL.iter().map(|i| i.shape()).collect();
        shapes.sort_unstable();
        shapes.dedup();
        assert_eq!(shapes.len(), Import::ALL.len());
        let mut keys: Vec<u64> = Import::ALL.iter().map(|i| import_key(i.name())).collect();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), Import::ALL.len());
    }

    #[test]
    fn the_section_reads_back_as_this_layout() {
        let layout = PluginLayout::parse(&LAYOUT_SECTION_BYTES).unwrap();
        assert_eq!(layout.core, CORE_SHAPE);
        for import in Import::ALL {
            assert_eq!(
                layout.import_shape(import.name()),
                Some(import.shape()),
                "{import:?}"
            );
        }
        assert_eq!(layout.import_shape("no_such_import"), None);
        // A section from an SDK whose table had fewer imports reads too.
        let older = PluginLayout::parse(&LAYOUT_SECTION_BYTES[..8 + 16]).unwrap();
        assert_eq!(
            older.import_shape(Import::ALL[0].name()),
            Some(Import::ALL[0].shape())
        );
        assert_eq!(older.import_shape(Import::ALL[1].name()), None);
    }

    #[test]
    fn a_malformed_section_is_none() {
        assert_eq!(PluginLayout::parse(b""), None);
        assert_eq!(PluginLayout::parse(b"1.0"), None);
        assert_eq!(PluginLayout::parse(&LAYOUT_SECTION_BYTES[..9]), None);
        assert_eq!(PluginLayout::parse(&LAYOUT_SECTION_BYTES[..8 + 15]), None);
    }
}
