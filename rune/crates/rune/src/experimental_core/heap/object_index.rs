//! Key-length-bounded critical-bit index. Leaves are positions in ordered fields.
//!
//! A byte is encoded as byte+1 in nine bits; zero terminates the key. Branch
//! bits strictly increase, so a query visits at most 9*(len+1)+1 links. Stop
//! before a branch beyond that terminator: even a short absent key cannot walk
//! an adversarial long-key subtree. No hashing, recursive walk or child roots.
use super::*;

pub(super) const INDEX_AT: usize = 32;

#[derive(Clone, Copy, Debug)]
struct Branch {
	bit: usize,
	child: [usize; 2],
	sample: usize,
}

#[derive(Debug)]
pub(super) struct Index {
	root: Option<usize>,
	nodes: Vec<Branch>,
}

#[derive(Clone, Copy)]
pub(super) enum Insert {
	First {
		entry: usize,
	},
	Branch {
		parent: Option<(usize, usize)>,
		old: usize,
		bit: usize,
		child: [usize; 2],
		sample: usize,
	},
}

fn leaf(entry: usize) -> usize {
	// Each entry occupies at least two words; allocatable field positions fit
	// in half a usize. The low bit distinguishes leaves from branch links.
	(entry << 1) | 1
}
fn bit(key: &str, at: usize) -> usize {
	let symbol = key.as_bytes().get(at / 9).map_or(0, |b| u16::from(*b) + 1);
	usize::from((symbol >> (8 - at % 9)) & 1)
}
fn first_diff(a: &str, b: &str) -> Option<usize> {
	for i in 0..=a.len().min(b.len()) {
		#[cfg(test)]
		probe_bytes(1);
		let a = a.as_bytes().get(i).map_or(0, |b| u16::from(*b) + 1);
		let b = b.as_bytes().get(i).map_or(0, |b| u16::from(*b) + 1);
		if a != b {
			return Some(i * 9 + (a ^ b).leading_zeros() as usize - 7);
		}
	}
	None
}

impl Index {
	pub(super) fn planned(capacity: usize) -> Result<usize> {
		capacity
			.saturating_sub(1)
			.checked_mul(size_of::<Branch>())
			.and_then(|n| n.checked_add(size_of::<Self>()))
			.ok_or_else(|| err(ErrorKind::Overflow))
	}
	pub(super) fn bytes(&self) -> usize {
		size_of::<Self>() + self.nodes.capacity() * size_of::<Branch>()
	}
	pub(super) fn room(&self) -> bool {
		self.nodes.len() < self.nodes.capacity()
	}
	pub(super) fn empty(capacity: usize) -> Result<Self> {
		Ok(Self {
			root: None,
			nodes: Vec::try_with_capacity(capacity.saturating_sub(1))?,
		})
	}
	pub(super) fn copy(&self, capacity: usize) -> Result<Self> {
		let mut next = Self::empty(capacity)?;
		#[cfg(test)]
		probe_copies(self.nodes.len());
		for node in &self.nodes {
			next.nodes
				.push_within_capacity(*node)
				.map_err(|_| err(ErrorKind::InvalidCode))?;
		}
		next.root = self.root;
		Ok(next)
	}
	pub(super) fn build<'a>(
		length: usize,
		capacity: usize,
		key: impl Fn(usize) -> &'a str,
	) -> Result<Self> {
		let mut index = Self::empty(capacity)?;
		for entry in 0..length {
			let insert = index
				.prepare(key(entry), entry, &key)
				.ok_or_else(|| err(ErrorKind::InvalidCode))?;
			index.commit(insert)?;
		}
		Ok(index)
	}
	pub(super) fn position<'a>(&self, key: &str, get: impl Fn(usize) -> &'a str) -> Option<usize> {
		let mut link = self.root?;
		let limit = key.len().saturating_add(1).saturating_mul(9);
		loop {
			#[cfg(test)]
			probe_link();
			if link & 1 != 0 {
				let entry = link >> 1;
				#[cfg(test)]
				probe_key();
				let other = get(entry);
				#[cfg(test)]
				probe_bytes(if other.len() == key.len() {
					key.len()
				} else {
					0
				});
				return (other == key).then_some(entry);
			}
			let node = &self.nodes[link >> 1];
			if node.bit >= limit {
				return None;
			}
			link = node.child[bit(key, node.bit)];
		}
	}
	pub(super) fn prepare<'a>(
		&self,
		key: &str,
		entry: usize,
		get: impl Fn(usize) -> &'a str,
	) -> Option<Insert> {
		let Some(mut link) = self.root else {
			return Some(Insert::First { entry });
		};
		let limit = key.len().saturating_add(1).saturating_mul(9);
		let sample = loop {
			#[cfg(test)]
			probe_link();
			if link & 1 != 0 {
				break link >> 1;
			}
			let node = &self.nodes[link >> 1];
			if node.bit >= limit {
				break node.sample;
			}
			link = node.child[bit(key, node.bit)];
		};
		#[cfg(test)]
		probe_key();
		let different = first_diff(key, get(sample))?;
		let mut parent = None;
		link = self.root?;
		while link & 1 == 0 && self.nodes[link >> 1].bit < different {
			#[cfg(test)]
			probe_link();
			let at = link >> 1;
			let direction = bit(key, self.nodes[at].bit);
			parent = Some((at, direction));
			link = self.nodes[at].child[direction];
		}
		let mut child = [link; 2];
		child[bit(key, different)] = leaf(entry);
		Some(Insert::Branch {
			old: link,
			parent,
			bit: different,
			child,
			sample,
		})
	}
	// All capacity and link proofs precede mutation. A capacity failure changes
	// nothing; after the append, only existing private links are overwritten.
	pub(super) fn commit(&mut self, insert: Insert) -> Result<()> {
		match insert {
			Insert::First { entry } => self.root = Some(leaf(entry)),
			Insert::Branch {
				old: _,
				parent,
				bit,
				child,
				sample,
			} => {
				let link = self.nodes.len() << 1;
				self.nodes
					.push_within_capacity(Branch { bit, child, sample })
					.map_err(|_| err(ErrorKind::InvalidCode))?;
				match parent {
					Some((at, direction)) => self.nodes[at].child[direction] = link,
					None => self.root = Some(link),
				}
			}
		}
		Ok(())
	}
	/// Undo a committed insertion if a later checked field-capacity proof fails.
	/// No key, value, ownership edge or allocation is held by this index.
	pub(super) fn undo(&mut self, insert: Insert) {
		match insert {
			Insert::First { .. } => self.root = None,
			Insert::Branch { parent, old, .. } => {
				match parent {
					Some((at, direction)) => self.nodes[at].child[direction] = old,
					None => self.root = Some(old),
				}
				self.nodes.pop();
			}
		}
	}

	pub(super) fn remove(&mut self, key: &str, entry: usize) {
		let Some(mut link) = self.root else {
			return;
		};
		let mut parent = None;
		let mut grand = None;
		while link & 1 == 0 {
			let at = link >> 1;
			let direction = bit(key, self.nodes[at].bit);
			grand = parent;
			parent = Some((at, direction));
			link = self.nodes[at].child[direction];
		}
		let Some((removed, direction)) = parent else {
			self.root = None;
			return;
		};
		let sibling = self.nodes[removed].child[1 - direction];
		let sample = if sibling & 1 != 0 {
			sibling >> 1
		} else {
			self.nodes[sibling >> 1].sample
		};
		match grand {
			Some((at, direction)) => self.nodes[at].child[direction] = sibling,
			None => self.root = Some(sibling),
		}
		let last = self.nodes.len() - 1;
		self.nodes.swap_remove(removed);
		let fix = |link: &mut usize| {
			if *link & 1 != 0 {
				let at = *link >> 1;
				if at > entry {
					*link = leaf(at - 1);
				}
			} else if *link >> 1 == last {
				*link = removed << 1;
			}
		};
		if let Some(root) = &mut self.root {
			fix(root);
		}
		for node in &mut self.nodes {
			for child in &mut node.child {
				fix(child);
			}
			if node.sample == entry {
				node.sample = sample;
			}
			if node.sample > entry {
				node.sample -= 1;
			}
		}
	}
}

#[cfg(target_pointer_width = "64")]
const _: () = {
	assert!(size_of::<Index>() == 40);
	assert!(size_of::<Branch>() == 32);
};
#[cfg(test)]
std::thread_local! { static PROBES: core::cell::Cell<[usize;4]> = const { core::cell::Cell::new([0;4]) }; }
#[cfg(test)]
fn probe_link() {
	PROBES.with(|p| {
		let mut v = p.get();
		v[0] += 1;
		p.set(v);
	});
}
#[cfg(test)]
fn probe_key() {
	PROBES.with(|p| {
		let mut v = p.get();
		v[1] += 1;
		p.set(v);
	});
}
#[cfg(test)]
fn probe_bytes(n: usize) {
	PROBES.with(|p| {
		let mut v = p.get();
		v[2] += n;
		p.set(v);
	});
}
#[cfg(test)]
pub(super) fn probe_copies(n: usize) {
	PROBES.with(|p| {
		let mut v = p.get();
		v[3] += n;
		p.set(v);
	});
}
#[cfg(test)]
pub(super) fn probes() -> [usize; 4] {
	PROBES.with(|p| p.replace([0; 4]))
}
