use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use ac_bits::Reader;

use crate::btree::{DatEntry, Node, is_valid_offset};
use crate::cache::{CacheStats, DEFAULT_NODE_CACHE_LIMIT, NodeCache};
use crate::error::{DatError, Result};
use crate::header::{DatFileType, DatHeader};
use crate::io::read_exact_at;
use crate::objects::{DatObject, Iteration};

/// File id of the iteration record every dat carries.
pub const ITERATION_ID: u32 = 0xFFFF_0001;

/// The most bytes one read of a block run takes (a file larger than this
/// is read in several runs).
const MAX_RUN_BYTES: usize = 1024 * 1024;

/// One open dat file: header, directory and positional reads.
///
/// `DatFile` is `Send + Sync`. Every read goes by file position through
/// `&self`; besides the header and the directory's root node it keeps a
/// bounded, thread-safe cache of the directory nodes lookups have read, so
/// any number of threads may read at once.
#[derive(Debug)]
pub struct DatFile {
    path: PathBuf,
    file: File,
    file_len: u64,
    header: DatHeader,
    root: Option<Arc<Node>>,
    nodes: NodeCache,
    iteration: Iteration,
}

impl DatFile {
    /// Open a dat file of any kind.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::open_with_node_cache(path, DEFAULT_NODE_CACHE_LIMIT)
    }

    /// Open a dat file keeping at most `node_limit` directory nodes besides
    /// the root (0: every lookup reads its nodes from the file).
    pub fn open_with_node_cache(path: impl AsRef<Path>, node_limit: usize) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let io_error = |source| DatError::Io {
            path: path.clone(),
            source,
        };
        let file = File::open(&path).map_err(io_error)?;
        let file_len = file.metadata().map_err(io_error)?.len();
        if file_len < DatHeader::SIZE as u64 {
            return Err(DatError::TooShort {
                path,
                len: file_len,
            });
        }
        let mut header_bytes = [0u8; DatHeader::SIZE];
        read_exact_at(&file, &mut header_bytes, 0).map_err(io_error)?;
        let header = DatHeader::unpack(&header_bytes)?;
        if header.magic != DatHeader::RETAIL_MAGIC {
            return Err(DatError::BadMagic {
                path,
                magic: header.magic,
            });
        }
        // A block must hold its 4-byte next pointer and at least one data byte.
        if header.block_size <= 4 {
            return Err(DatError::BadBlockSize {
                path,
                block_size: header.block_size,
            });
        }

        let mut dat = Self {
            path,
            file,
            file_len,
            header,
            root: None,
            nodes: NodeCache::new(node_limit),
            iteration: Iteration::default(),
        };
        if dat.header.root_block != 0 {
            dat.root = Some(Arc::new(dat.read_node(dat.header.root_block)?));
        }
        if let Some(iteration) = dat.get::<Iteration>(ITERATION_ID)? {
            dat.iteration = iteration;
        }
        Ok(dat)
    }

    /// Open a dat file and check that its header names the expected kind.
    pub fn open_as(path: impl AsRef<Path>, expected: DatFileType) -> Result<Self> {
        let dat = Self::open(path)?;
        let found = dat.header.file_type;
        if found != expected {
            return Err(DatError::WrongType {
                path: dat.path,
                expected,
                found,
            });
        }
        Ok(dat)
    }

    /// Path the file was opened from.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Length of the file on disk, in bytes.
    #[must_use]
    pub fn file_len(&self) -> u64 {
        self.file_len
    }

    /// The decoded header.
    #[must_use]
    pub fn header(&self) -> &DatHeader {
        &self.header
    }

    /// The iteration record, or the default (iteration 1) when the dat has none.
    #[must_use]
    pub fn iteration(&self) -> &Iteration {
        &self.iteration
    }

    /// Look up a file's directory entry by id.
    pub fn find_entry(&self, id: u32) -> Result<Option<DatEntry>> {
        let mut block = self.header.root_block;
        while is_valid_offset(block) {
            let node = self.cached_node(block)?;
            match node.search(id) {
                Ok(index) => return Ok(Some(node.files[index])),
                Err(_) if node.is_leaf() => break,
                Err(child) => block = node.child(child),
            }
        }
        Ok(None)
    }

    /// Every directory entry, in id order.
    #[must_use]
    pub fn entries(&self) -> Entries<'_> {
        Entries {
            dat: self,
            stack: self
                .root
                .iter()
                .map(|root| Frame {
                    node: Node::clone(root),
                    next: 0,
                    descended: false,
                })
                .collect(),
        }
    }

    /// Read the stored bytes of an entry.
    pub fn read_entry(&self, entry: &DatEntry) -> Result<Vec<u8>> {
        if entry.is_compressed() {
            return Err(DatError::Compressed { id: entry.id });
        }
        if u64::from(entry.size) > self.file_len {
            return Err(DatError::EntryTooLarge {
                id: entry.id,
                size: entry.size,
            });
        }
        self.read_chain(entry.offset, entry.size as usize)
    }

    /// Read a file's bytes by id; `None` when the directory has no such file.
    pub fn get_bytes(&self, id: u32) -> Result<Option<Vec<u8>>> {
        match self.find_entry(id)? {
            Some(entry) => self.read_entry(&entry).map(Some),
            None => Ok(None),
        }
    }

    /// Read and decode a typed object by id; `None` when the directory has
    /// no such file. The object's id is set to the file id it was read from.
    pub fn get<T: DatObject>(&self, id: u32) -> Result<Option<T>> {
        let Some(bytes) = self.get_bytes(id)? else {
            return Ok(None);
        };
        let mut object = T::unpack(&mut Reader::new(&bytes))
            .map_err(|source| DatError::Decode { id, source })?;
        object.set_id(id);
        Ok(Some(object))
    }

    /// Hits, misses and evictions of the directory-node cache.
    #[must_use]
    pub fn node_cache_stats(&self) -> CacheStats {
        self.nodes.stats()
    }

    /// Directory nodes the cache holds (the root is kept apart).
    #[must_use]
    pub fn cached_nodes(&self) -> usize {
        self.nodes.len()
    }

    /// The node at `offset`: the root, a cached node, or one read now and
    /// kept. A node that fails to decode is not kept.
    fn cached_node(&self, offset: i32) -> Result<Arc<Node>> {
        if let Some(root) = &self.root
            && root.offset == offset
        {
            return Ok(root.clone());
        }
        if let Some(node) = self.nodes.get(offset) {
            return Ok(node);
        }
        Ok(self.nodes.insert(Arc::new(self.read_node(offset)?)))
    }

    fn read_node(&self, offset: i32) -> Result<Node> {
        let bytes = self.read_chain(offset, Node::DISK_SIZE)?;
        Node::unpack(offset, &bytes)
    }

    /// Gather `len` bytes from the block chain starting at `start`.
    ///
    /// Each block begins with the offset of the next block (0 ends the
    /// chain); the remaining `block_size - 4` bytes are data. Only the bytes
    /// still needed are used from the last block.
    ///
    /// A file's blocks almost always follow one another on disk (in the
    /// retail dats about one block in 200 jumps elsewhere), so the blocks
    /// still needed are read as one run with one system call, and a block
    /// is read again from the file only when the chain leaves the run.
    fn read_chain(&self, start: i32, len: usize) -> Result<Vec<u8>> {
        let block_size = self.header.block_size as usize;
        let data_per_block = block_size - 4;
        let mut out = vec![0u8; len];
        let mut filled = 0;
        let mut block = start;
        let mut run: Vec<u8> = Vec::new();
        let mut run_start = 0u64;
        while block != 0 && filled < len {
            let take = data_per_block.min(len - filled);
            let need = (4 + take) as u64;
            let Some(offset) = u64::try_from(block)
                .ok()
                .filter(|&offset| offset + need <= self.file_len)
            else {
                return Err(DatError::BlockOutOfRange {
                    offset: i64::from(block),
                    file_len: self.file_len,
                });
            };
            let in_run = offset >= run_start && offset + need <= run_start + run.len() as u64;
            if !in_run {
                let blocks_left = (len - filled).div_ceil(data_per_block);
                let run_len = (blocks_left * block_size)
                    .min(MAX_RUN_BYTES)
                    .min(usize::try_from(self.file_len - offset).unwrap_or(usize::MAX))
                    .max(4 + take);
                run.resize(run_len, 0);
                read_exact_at(&self.file, &mut run, offset).map_err(|source| DatError::Io {
                    path: self.path.clone(),
                    source,
                })?;
                run_start = offset;
            }
            let at = (offset - run_start) as usize;
            let span = &run[at..at + 4 + take];
            out[filled..filled + take].copy_from_slice(&span[4..]);
            filled += take;
            block = i32::from_le_bytes([span[0], span[1], span[2], span[3]]);
        }
        if filled < len {
            return Err(DatError::ChainEnded {
                start,
                needed: len,
                read: filled,
            });
        }
        Ok(out)
    }
}

/// In-order walk of a dat's directory; see [`DatFile::entries`].
///
/// Yields an error and stops if a node cannot be read.
#[derive(Debug)]
pub struct Entries<'a> {
    dat: &'a DatFile,
    stack: Vec<Frame>,
}

#[derive(Debug)]
struct Frame {
    node: Node,
    /// Next entry to yield.
    next: usize,
    /// Whether the child before entry `next` has been walked.
    descended: bool,
}

impl Iterator for Entries<'_> {
    type Item = Result<DatEntry>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let top = self.stack.last_mut()?;
            if !top.node.is_leaf() && !top.descended {
                top.descended = true;
                let child = top.node.child(top.next);
                let parent = top.node.offset;
                if !is_valid_offset(child) {
                    self.stack.clear();
                    return Some(Err(DatError::BadChild {
                        offset: parent,
                        child,
                    }));
                }
                match self.dat.read_node(child) {
                    Ok(node) => self.stack.push(Frame {
                        node,
                        next: 0,
                        descended: false,
                    }),
                    Err(error) => {
                        self.stack.clear();
                        return Some(Err(error));
                    }
                }
                continue;
            }
            if let Some(&entry) = top.node.files.get(top.next) {
                top.next += 1;
                top.descended = false;
                return Some(Ok(entry));
            }
            self.stack.pop();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::btree::tests::{entry, node_bytes};
    use std::io::Write;

    const _: () = {
        const fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<DatFile>();
    };

    /// A tiny in-memory dat builder: 64-byte blocks, header in the first
    /// blocks, then whatever chains the test lays out.
    struct DatBuilder {
        bytes: Vec<u8>,
        block_size: usize,
    }

    impl DatBuilder {
        fn new(block_size: usize) -> Self {
            let header_blocks = DatHeader::SIZE.div_ceil(block_size);
            Self {
                bytes: vec![0; header_blocks * block_size],
                block_size,
            }
        }

        /// Append `data` as a fresh chain; returns its first block offset.
        fn chain(&mut self, data: &[u8]) -> i32 {
            let per = self.block_size - 4;
            let blocks = data.len().div_ceil(per).max(1);
            let start = self.bytes.len();
            for i in 0..blocks {
                let next = if i + 1 < blocks {
                    (start + (i + 1) * self.block_size) as i32
                } else {
                    0
                };
                self.bytes.extend_from_slice(&next.to_le_bytes());
                let chunk = &data[(i * per).min(data.len())..((i + 1) * per).min(data.len())];
                self.bytes.extend_from_slice(chunk);
                self.bytes.extend(std::iter::repeat_n(0, per - chunk.len()));
            }
            start as i32
        }

        /// Append `data` as a chain whose blocks lie on disk in reverse
        /// order (every next pointer jumps backwards); returns its first
        /// block offset.
        fn scattered_chain(&mut self, data: &[u8]) -> i32 {
            let per = self.block_size - 4;
            let blocks = data.len().div_ceil(per).max(1);
            let base = self.bytes.len();
            let at = |i: usize| base + (blocks - 1 - i) * self.block_size;
            self.bytes.resize(base + blocks * self.block_size, 0);
            for i in 0..blocks {
                let next = if i + 1 < blocks { at(i + 1) as i32 } else { 0 };
                let chunk = &data[(i * per).min(data.len())..((i + 1) * per).min(data.len())];
                let o = at(i);
                self.bytes[o..o + 4].copy_from_slice(&next.to_le_bytes());
                self.bytes[o + 4..o + 4 + chunk.len()].copy_from_slice(chunk);
            }
            at(0) as i32
        }

        fn finish(mut self, file_type: u32, root: i32) -> Vec<u8> {
            let mut header = crate::header::tests::sample_header_bytes();
            header[324..328].copy_from_slice(&(self.block_size as i32).to_le_bytes());
            header[328..332].copy_from_slice(&(self.bytes.len() as i32).to_le_bytes());
            header[332..336].copy_from_slice(&file_type.to_le_bytes());
            header[352..356].copy_from_slice(&root.to_le_bytes());
            self.bytes[..DatHeader::SIZE].copy_from_slice(&header);
            self.bytes
        }
    }

    fn write_temp(name: &str, bytes: &[u8]) -> PathBuf {
        let path =
            std::env::temp_dir().join(format!("ac-dat-test-{}-{name}.dat", std::process::id()));
        let mut file = File::create(&path).unwrap();
        file.write_all(bytes).unwrap();
        path
    }

    /// A two-level directory: root with entries 20 and 40, three leaves.
    fn two_level_dat() -> (Vec<u8>, Vec<u32>) {
        let mut b = DatBuilder::new(64);
        let payload: Vec<u8> = (0u8..150).collect();
        let data = b.chain(&payload);
        let small = b.chain(b"hello");
        let leaf =
            |ids: &[u32]| -> Vec<DatEntry> { ids.iter().map(|&id| entry(id, small, 5)).collect() };
        let left = b.chain(&node_bytes(&[], &leaf(&[5, 10])));
        let mid = b.chain(&node_bytes(&[], &[entry(30, data, 150)]));
        let right = b.chain(&node_bytes(&[], &leaf(&[50, 60, 70])));
        let root = b.chain(&node_bytes(
            &[left, mid, right],
            &[entry(20, small, 5), entry(40, small, 5)],
        ));
        (b.finish(2, root), vec![5, 10, 20, 30, 40, 50, 60, 70])
    }

    #[test]
    fn finds_entries_at_every_level_and_reads_chains() {
        let (bytes, ids) = two_level_dat();
        let path = write_temp("two-level", &bytes);
        let dat = DatFile::open_as(&path, DatFileType::Cell).unwrap();
        for id in &ids {
            assert_eq!(dat.find_entry(*id).unwrap().map(|e| e.id), Some(*id));
        }
        for missing in [0, 1, 15, 25, 45, 80, u32::MAX] {
            assert_eq!(dat.find_entry(missing).unwrap(), None);
        }
        // 150 bytes across three 60-byte data blocks.
        let bytes30 = dat.get_bytes(30).unwrap().unwrap();
        assert_eq!(bytes30, (0u8..150).collect::<Vec<_>>());
        assert_eq!(dat.get_bytes(60).unwrap().unwrap(), b"hello");
        assert_eq!(dat.get_bytes(61).unwrap(), None);
        // No iteration record: the default.
        assert_eq!(dat.iteration().current_iteration, 1);
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn repeated_lookups_read_cached_nodes() {
        let (bytes, ids) = two_level_dat();
        let path = write_temp("node-cache", &bytes);
        let dat = DatFile::open(&path).unwrap();
        // Lookups of 5 and 50 visit the root (kept apart) and one leaf each.
        for id in [5, 50] {
            dat.find_entry(id).unwrap();
        }
        assert_eq!(dat.cached_nodes(), 2);
        let before = dat.node_cache_stats();
        for id in &ids {
            assert_eq!(dat.find_entry(*id).unwrap().map(|e| e.id), Some(*id));
        }
        let after = dat.node_cache_stats();
        // 20 and 40 sit in the root; the other six ids each read one leaf,
        // and only the middle leaf (30) is new.
        assert_eq!(after.hits - before.hits, 5);
        assert_eq!(after.misses - before.misses, 1);
        assert_eq!(dat.cached_nodes(), 3);
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn the_node_cache_stays_within_its_limit_and_answers_the_same() {
        let (bytes, ids) = two_level_dat();
        let path = write_temp("node-limit", &bytes);
        let uncached = DatFile::open_with_node_cache(&path, 0).unwrap();
        let one = DatFile::open_with_node_cache(&path, 1).unwrap();
        for _ in 0..3 {
            for id in ids.iter().copied().chain([0, 15, 45, 80, u32::MAX]) {
                assert_eq!(
                    one.find_entry(id).unwrap(),
                    uncached.find_entry(id).unwrap()
                );
                assert!(one.cached_nodes() <= 1);
            }
        }
        assert_eq!(uncached.cached_nodes(), 0);
        assert!(one.node_cache_stats().evictions > 0);
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn concurrent_lookups_through_a_small_node_cache_agree() {
        let (bytes, ids) = two_level_dat();
        let path = write_temp("node-threads", &bytes);
        let dat = DatFile::open_with_node_cache(&path, 2).unwrap();
        let reference = DatFile::open_with_node_cache(&path, 0).unwrap();
        let expected: Vec<_> = (0..80)
            .map(|id| reference.find_entry(id).unwrap())
            .collect();
        std::thread::scope(|scope| {
            for t in 0..8u32 {
                let (dat, expected) = (&dat, &expected);
                scope.spawn(move || {
                    for round in 0..200u32 {
                        let id = (t * 7 + round * 13) % 80;
                        assert_eq!(dat.find_entry(id).unwrap(), expected[id as usize]);
                        assert!(dat.cached_nodes() <= 2);
                    }
                });
            }
        });
        for id in &ids {
            assert_eq!(
                dat.get_bytes(*id).unwrap(),
                reference.get_bytes(*id).unwrap()
            );
        }
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn entries_walk_in_id_order() {
        let (bytes, ids) = two_level_dat();
        let path = write_temp("walk", &bytes);
        let dat = DatFile::open(&path).unwrap();
        let walked: Vec<u32> = dat.entries().map(|e| e.unwrap().id).collect();
        assert_eq!(walked, ids);
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn empty_directory_has_no_entries() {
        let b = DatBuilder::new(64);
        let path = write_temp("empty", &b.finish(1, 0));
        let dat = DatFile::open(&path).unwrap();
        assert_eq!(dat.entries().count(), 0);
        assert_eq!(dat.find_entry(1).unwrap(), None);
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn wrong_type_bad_magic_and_short_files_are_errors() {
        let (mut bytes, _) = two_level_dat();
        let path = write_temp("wrong-type", &bytes);
        assert!(matches!(
            DatFile::open_as(&path, DatFileType::Portal),
            Err(DatError::WrongType {
                found: DatFileType::Cell,
                ..
            })
        ));
        bytes[320] = 0;
        let path2 = write_temp("bad-magic", &bytes);
        assert!(matches!(
            DatFile::open(&path2),
            Err(DatError::BadMagic { .. })
        ));
        let path3 = write_temp("short", &bytes[..100]);
        assert!(matches!(
            DatFile::open(&path3),
            Err(DatError::TooShort { len: 100, .. })
        ));
        for p in [path, path2, path3] {
            std::fs::remove_file(p).ok();
        }
    }

    #[test]
    fn broken_chains_are_errors() {
        let mut b = DatBuilder::new(64);
        let data = b.chain(b"abc");
        let leaf = b.chain(&node_bytes(
            &[],
            &[
                entry(1, data, 3),
                entry(2, data, 200),      // chain too short
                entry(3, 0x7FFF_0000, 3), // past the end of the file
                entry(4, -64, 3),         // negative offset
                entry(5, data, u32::MAX), // larger than the file
            ],
        ));
        let path = write_temp("broken", &b.finish(1, leaf));
        let dat = DatFile::open(&path).unwrap();
        assert_eq!(dat.get_bytes(1).unwrap().unwrap(), b"abc");
        assert!(matches!(
            dat.get_bytes(2),
            Err(DatError::ChainEnded {
                needed: 200,
                read: 60,
                ..
            })
        ));
        assert!(matches!(
            dat.get_bytes(3),
            Err(DatError::BlockOutOfRange { .. })
        ));
        assert!(matches!(
            dat.get_bytes(4),
            Err(DatError::BlockOutOfRange { .. })
        ));
        assert!(matches!(
            dat.get_bytes(5),
            Err(DatError::EntryTooLarge { .. })
        ));
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn chains_that_jump_and_chains_at_the_end_of_the_file_read_whole() {
        let mut b = DatBuilder::new(64);
        let long: Vec<u8> = (0..1000u32).map(|i| (i * 7 % 251) as u8).collect();
        let scattered = b.scattered_chain(&long);
        let contiguous = b.chain(&long);
        // The leaf comes next; the last chain of the file follows it.
        let leaf_len = Node::DISK_SIZE.div_ceil(60) * 64;
        let tail = (b.bytes.len() + leaf_len) as i32;
        let leaf = b.chain(&node_bytes(
            &[],
            &[
                entry(1, scattered, 1000),
                entry(2, contiguous, 1000),
                entry(3, tail, 10),
            ],
        ));
        assert_eq!(b.chain(b"0123456789"), tail);
        // The file ends right after the bytes the tail's block holds.
        let mut bytes = b.finish(1, leaf);
        bytes.truncate(tail as usize + 4 + 10);
        let path = write_temp("runs", &bytes);
        let dat = DatFile::open(&path).unwrap();
        assert_eq!(dat.get_bytes(1).unwrap().unwrap(), long);
        assert_eq!(dat.get_bytes(2).unwrap().unwrap(), long);
        assert_eq!(dat.get_bytes(3).unwrap().unwrap(), b"0123456789");
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn compressed_entries_are_refused() {
        let mut b = DatBuilder::new(64);
        let data = b.chain(b"abc");
        let mut e = entry(1, data, 3);
        e.flags = 1;
        let leaf = b.chain(&node_bytes(&[], &[e]));
        let path = write_temp("compressed", &b.finish(1, leaf));
        let dat = DatFile::open(&path).unwrap();
        assert!(matches!(
            dat.get_bytes(1),
            Err(DatError::Compressed { id: 1 })
        ));
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn walk_reports_an_invalid_child() {
        let mut b = DatBuilder::new(64);
        let leaf = b.chain(&node_bytes(&[], &[entry(1, 0, 0)]));
        // Two entries but the second child slot is the fill pattern.
        let root = b.chain(&node_bytes(
            &[leaf, 0xCDCD_CDCDu32 as i32],
            &[entry(5, 0, 0), entry(9, 0, 0)],
        ));
        let path = write_temp("bad-child", &b.finish(1, root));
        let dat = DatFile::open(&path).unwrap();
        let walked: Vec<_> = dat.entries().collect();
        assert_eq!(walked.len(), 3);
        assert_eq!(walked[0].as_ref().unwrap().id, 1);
        assert_eq!(walked[1].as_ref().unwrap().id, 5);
        assert!(matches!(
            walked[2],
            Err(DatError::BadChild { child: 0, .. })
        ));
        // Lookup down the missing child simply finds nothing.
        assert_eq!(dat.find_entry(7).unwrap(), None);
        std::fs::remove_file(path).ok();
    }
}
