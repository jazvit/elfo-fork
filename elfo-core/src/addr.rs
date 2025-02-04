use std::{
    fmt,
    num::{NonZeroU16, NonZeroU8},
};

use derive_more::Display;
use serde::{Deserialize, Serialize};

// === NodeNo ===

/// Represents the node's number.
/// Cannot be `0`, it's reserved to represent the local node.
#[stability::unstable]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[derive(Display, Serialize, Deserialize)]
pub struct NodeNo(NonZeroU16);

impl NodeNo {
    #[stability::unstable]
    #[inline]
    pub fn from_bits(bits: u16) -> Option<Self> {
        NonZeroU16::new(bits).map(NodeNo)
    }

    #[stability::unstable]
    #[inline]
    pub fn into_bits(self) -> u16 {
        self.0.get()
    }
}

// === NodeLaunchId ===

/// Randomly generated identifier at the node start.
///
/// Used for several purposes:
/// * To distinguish between different launches of the same node.
/// * To detect reusing of the same node no.
/// * To improve [`Addr`] uniqueness in the cluster.
#[stability::unstable]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Display)]
pub struct NodeLaunchId(u64);

impl NodeLaunchId {
    pub(crate) fn generate() -> Self {
        use std::{
            collections::hash_map::RandomState,
            hash::{BuildHasher, Hasher},
        };

        // `RandomState` is randomly seeded.
        let mut hasher = RandomState::new().build_hasher();
        hasher.write_u64(0xE1F0E1F0E1F0E1F0);
        Self(hasher.finish())
    }

    #[stability::unstable]
    #[inline]
    pub fn from_bits(bits: u64) -> Self {
        Self(bits)
    }

    #[stability::unstable]
    #[inline]
    pub fn into_bits(self) -> u64 {
        self.0
    }
}

// === GroupNo ===

/// Represents the actor group's number.
///
/// Cannot be `0`, it's reserved to represent `Addr::NULL` unambiguously.
/// XORed with random [`NodeLaunchId`] if the `network` feature is enabled.
#[stability::unstable]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[derive(Display, Serialize, Deserialize)]
pub struct GroupNo(NonZeroU8);

impl GroupNo {
    #[cfg(feature = "network")]
    pub(crate) fn new(no: u8, launch_id: NodeLaunchId) -> Option<Self> {
        if no == 0 {
            return None;
        }

        let xor = (launch_id.into_bits() >> GROUP_NO_SHIFT) as u8;

        // `no = 0` is forbidden, thus there is no mapping to just `xor`.
        let group_no = if no != xor { no ^ xor } else { xor };

        Some(Self(NonZeroU8::new(group_no).unwrap()))
    }

    #[cfg(not(feature = "network"))]
    pub(crate) fn new(no: u8, _launch_id: NodeLaunchId) -> Option<Self> {
        NonZeroU8::new(no).map(Self)
    }

    #[stability::unstable]
    #[inline]
    pub fn from_bits(bits: u8) -> Option<Self> {
        NonZeroU8::new(bits).map(Self)
    }

    #[stability::unstable]
    #[inline]
    pub fn into_bits(self) -> u8 {
        self.0.get()
    }
}

// === Addr ===

/// Represents the global, usually unique address of an actor or a group.
///
/// # Uniqueness
///
/// An address is based on a sharded slab to make it a simple sendable number
/// (as opposed to reference counting) and provide better performance of lookups
/// than hashmaps. However, it means deletions and insertions to the same
/// underlying slot multiple times can lead to reusing the address for a
/// different actor.
///
/// Elfo tries to do its best to ensure the uniqueness of this value:
/// * Alive actors on the same node always have different addresses.
/// * Actors in different nodes have different address spaces.
/// * Actors in different groups have different address spaces.
/// * An address includes the version number to guard against the ABA problem.
/// * An address is randomized between restarts of the same node if the
///   `network` feature is enabled.
///
/// # Using addresses in messages
///
/// The current implementation of network depends on the fact that
/// `Addr` cannot be sent inside messages. It prevents from different
/// possible errors like responding without having a valid connection.
/// The only way to get an address of remote actor is `envelope.sender()`.
/// If sending `Addr` inside a message is unavoidable, use `Local<Addr>`,
/// however it won't be possible to send such message to a remote actor.
// ~
// Structure (64b platform):
//  64           48         40           30      21                0
//  +------------+----------+------------+-------+-----------------+
//  |   node_no  | group_no | generation |  TID  |  page + offset  |
//  |     16b    |    8b    |     10b    |   9b  |       21b       |
//  +------------+----------+------------+-------+-----------------+
//   (0 if local)           ^----------- slot key (40b) -----------^
//
// Structure (32b platform):
//  64           48         40     32       25     18              0
//  +------------+----------+------+--------+------+---------------+
//  |   node_no  | group_no | rand | genera | TID  | page + offset |
//  |     16b    |    8b    |  8b  |   7b   |  7b  |      18b      |
//  +------------+----------+------+--------+------+---------------+
//   (0 if local)                   ^------- slot key (32b) -------^
//
// Limits:                                         64b       32b
// - max nodes in a cluster                      65535     65535 (1)
// - max groups in a node                          255       255 (2, 3)
// - max active actors spawned by one thread   1048544    131040
// - slot generations to prevent ABA              1024       128
// - max threads spawning actors                   256        64
//
// 1. `0` is reserved to represent the local node.
// 2. `0` is reserved to represent `Addr::NULL` unambiguously.
// 3. at least one group (for `system.init`) is always present.
//
// If the `network` feature is enabled, bottom 48 bits are XORed with the current node's launch
// number, generated at startup. It ensures that the same actor on different launches of the same
// node will have different addresses. The original address is never printed or even represented
// and the slot key part is restored only by calling private `Addr::slot_key(launch_no)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Addr(u64); // TODO: make it `NonZeroU64` instead of `Addr::NULL`?

const NODE_NO_SHIFT: u32 = 48;
const GROUP_NO_SHIFT: u32 = 40;

// See `Addr` docs for details.
assert_not_impl_all!(Addr: Serialize, Deserialize<'static>);

impl fmt::Display for Addr {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_null() {
            return f.write_str("null");
        }

        let group_no = self.group_no().expect("invalid addr");
        let bottom = self.0 & ((1 << GROUP_NO_SHIFT) - 1);

        if let Some(node_no) = self.node_no() {
            write!(f, "{}/{}/{}", node_no, group_no, bottom)
        } else {
            write!(f, "{}/{}", group_no, bottom)
        }
    }
}

impl Addr {
    #[stability::unstable]
    pub const NULL: Addr = Addr(0);

    #[cfg(feature = "network")]
    pub(crate) fn new_local(slot_key: usize, group_no: GroupNo, launch_id: NodeLaunchId) -> Self {
        debug_assert!(slot_key < (1 << GROUP_NO_SHIFT));
        let slot_key = ((slot_key as u64) ^ launch_id.into_bits()) & ((1 << GROUP_NO_SHIFT) - 1);
        Self::new_local_inner(slot_key, group_no)
    }

    #[cfg(not(feature = "network"))]
    pub(crate) fn new_local(slot_key: usize, group_no: GroupNo, _launch_id: NodeLaunchId) -> Self {
        debug_assert!(slot_key < (1 << GROUP_NO_SHIFT));
        Self::new_local_inner(slot_key as u64, group_no)
    }

    fn new_local_inner(slot_key: u64, group_no: GroupNo) -> Self {
        Self(u64::from(group_no.into_bits()) << GROUP_NO_SHIFT | slot_key)
    }

    #[stability::unstable]
    #[inline]
    pub fn from_bits(bits: u64) -> Option<Self> {
        Some(Self(bits)).filter(|addr| addr.is_null() ^ addr.group_no().is_some())
    }

    #[stability::unstable]
    #[inline]
    pub fn into_bits(self) -> u64 {
        self.0
    }

    #[inline]
    pub fn is_null(self) -> bool {
        self == Self::NULL
    }

    #[inline]
    pub fn is_local(self) -> bool {
        !self.is_null() && self.node_no().is_none()
    }

    #[cfg(feature = "network")]
    #[inline]
    pub fn is_remote(self) -> bool {
        self.node_no().is_some()
    }

    #[stability::unstable]
    #[inline]
    pub fn node_no(self) -> Option<NodeNo> {
        NodeNo::from_bits((self.0 >> NODE_NO_SHIFT) as u16)
    }

    #[stability::unstable]
    #[inline]
    pub fn group_no(self) -> Option<GroupNo> {
        GroupNo::from_bits((self.0 >> GROUP_NO_SHIFT) as u8)
    }

    #[cfg(feature = "network")]
    pub(crate) fn node_no_group_no(self) -> u32 {
        (self.0 >> GROUP_NO_SHIFT) as u32
    }

    #[cfg(feature = "network")]
    pub(crate) fn slot_key(self, launch_id: NodeLaunchId) -> usize {
        // sharded-slab uses the lower bits only, so we can xor the whole address.
        (self.0 ^ launch_id.into_bits()) as usize
    }

    #[cfg(not(feature = "network"))]
    pub(crate) fn slot_key(self, _launch_id: NodeLaunchId) -> usize {
        self.0 as usize
    }

    #[cfg(feature = "network")]
    #[stability::unstable]
    #[inline]
    pub fn into_remote(self, node_no: NodeNo) -> Self {
        if self.is_local() {
            Self(self.0 | (node_no.into_bits() as u64) << NODE_NO_SHIFT)
        } else {
            self
        }
    }

    #[stability::unstable]
    #[inline]
    pub fn into_local(self) -> Self {
        Self(self.0 & ((1 << NODE_NO_SHIFT) - 1))
    }
}

// === SlabConfig ===

// Actually, it doesn't reexported.
pub struct SlabConfig;

#[cfg(target_pointer_width = "64")]
impl sharded_slab::Config for SlabConfig {
    const INITIAL_PAGE_SIZE: usize = 32;
    const MAX_PAGES: usize = 15;
    const MAX_THREADS: usize = 256;
    const RESERVED_BITS: usize = 24;
}
#[cfg(target_pointer_width = "64")]
const_assert_eq!(
    sharded_slab::Slab::<crate::object::Object, SlabConfig>::USED_BITS,
    GROUP_NO_SHIFT as usize
);

#[cfg(target_pointer_width = "32")]
impl sharded_slab::Config for SlabConfig {
    const INITIAL_PAGE_SIZE: usize = 32;
    const MAX_PAGES: usize = 12;
    const MAX_THREADS: usize = 64;
    const RESERVED_BITS: usize = 0;
}

#[cfg(target_pointer_width = "32")]
const_assert_eq!(Slab::<Object, SlabConfig>::USED_BITS, 32);

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use proptest::prelude::*;

    use super::*;

    #[test]
    fn node_launch_id_generate() {
        let count = 10;
        let set: HashSet<_> = (0..count).map(|_| NodeLaunchId::generate()).collect();
        assert_eq!(set.len(), count);
    }

    #[test]
    fn group_no() {
        let launch_ids = (0..5)
            .map(|_| NodeLaunchId::generate())
            .chain(Some(NodeLaunchId::from_bits(0)))
            .collect::<Vec<_>>();

        for launch_id in launch_ids {
            // no = 0 is always invalid.
            assert_eq!(GroupNo::new(0, launch_id), None);

            // `GroupNo` is unique for any `NodeLaunchId`.
            let set = (1..=u8::MAX)
                .map(|no| GroupNo::new(no, launch_id).unwrap())
                .collect::<HashSet<_>>();

            assert_eq!(set.len(), usize::from(u8::MAX));
        }
    }

    proptest! {
        #[test]
        fn addr(
            slot_keys in prop::collection::hash_set(0u64..(1 << GROUP_NO_SHIFT), 10),
            group_nos in prop::collection::hash_set(1..=u8::MAX, 10),
            launch_ids in prop::collection::hash_set(prop::num::u64::ANY, 10),
        ) {
            #[cfg(feature = "network")]
            let expected_count = slot_keys.len() * group_nos.len() * launch_ids.len();
            #[cfg(not(feature = "network"))]
            let expected_count = slot_keys.len() * group_nos.len();

            let mut set = HashSet::with_capacity(expected_count);

            for slot_key in &slot_keys {
                for group_no in &group_nos {
                    for launch_id in &launch_ids {
                        let slot_key = *slot_key as usize;
                        let launch_id = NodeLaunchId::from_bits(*launch_id);
                        let group_no = GroupNo::new(*group_no, launch_id).unwrap();
                        let addr = Addr::new_local(slot_key, group_no, launch_id);
                        set.insert(addr);

                        prop_assert!(!addr.is_null());
                        prop_assert!(addr.is_local());
                        prop_assert_eq!(addr.group_no(), Some(group_no));
                        prop_assert_eq!(addr.node_no(), None);
                        prop_assert_eq!(addr.slot_key(launch_id) & ((1 << GROUP_NO_SHIFT) - 1), slot_key);
                        prop_assert_eq!(addr.into_local(), addr);
                        prop_assert_eq!(Addr::from_bits(addr.into_bits()), Some(addr));
                        prop_assert_eq!(addr.to_string().split('/').count(), 2);
                        prop_assert!(addr.to_string().starts_with(&group_no.to_string()));

                        #[cfg(feature = "network")]
                        {
                            prop_assert!(!addr.is_remote());
                            let node_no = NodeNo::from_bits(42).unwrap();
                            let remote = addr.into_remote(node_no);
                            prop_assert!(!remote.is_null());
                            prop_assert!(!remote.is_local());
                            prop_assert!(remote.is_remote());
                            prop_assert_eq!(remote.group_no(), Some(group_no));
                            prop_assert_eq!(remote.node_no(), Some(node_no));
                            prop_assert_eq!(addr.into_local(), addr);
                            prop_assert_eq!(remote.node_no_group_no() >> 8, u32::from(node_no.into_bits()));
                            prop_assert_eq!(remote.node_no_group_no() & 0xff, u32::from(group_no.into_bits()));
                            prop_assert_eq!(remote.to_string().split('/').count(), 3);
                            prop_assert!(remote.to_string().starts_with(&node_no.to_string()));
                        }
                    }
                }
            }

            // Check uniqueness.
            prop_assert_eq!(set.len(), expected_count);
        }
    }

    #[test]
    fn addr_null() {
        let null = Addr::NULL;
        assert_eq!(null.to_string(), "null");
        assert!(null.is_null());
        assert_eq!(null.into_local(), null);
        assert_eq!(null.group_no(), None);
        assert_eq!(null.node_no(), None);
        #[cfg(feature = "network")]
        {
            assert!(!null.is_remote());
            assert_eq!(null.into_remote(NodeNo::from_bits(42).unwrap()), null);
            assert_eq!(null.node_no_group_no(), 0);
        }
    }

    #[test]
    fn addr_invalid() {
        assert_eq!(Addr::from_bits(1), None);
    }
}
