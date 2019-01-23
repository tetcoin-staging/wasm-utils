#![warn(missing_docs)]

use std::rc::Rc;
use std::cell::RefCell;
use std::vec::Vec;
use std::slice;

#[derive(Debug)]
enum EntryOrigin {
	Index(usize),
	Detached,
}

impl From<usize> for EntryOrigin {
	fn from(v: usize) -> Self {
		EntryOrigin::Index(v)
	}
}

/// Reference counting, link-handling object.
#[derive(Debug)]
pub struct Entry<T> {
	val: T,
	index: EntryOrigin,
}

impl<T> Entry<T> {
	/// New entity.
	pub fn new(val: T, index: usize) -> Entry<T> {
		Entry {
			val: val,
			index: EntryOrigin::Index(index),
		}
	}

	/// Index of the element within the reference list.
	pub fn order(&self) -> Option<usize> {
		match self.index {
			EntryOrigin::Detached => None,
			EntryOrigin::Index(idx) => Some(idx),
		}
	}
}

impl<T> ::std::ops::Deref for Entry<T> {
	type Target = T;

	fn deref(&self) -> &T {
		&self.val
	}
}

impl<T> ::std::ops::DerefMut for Entry<T> {
	fn deref_mut(&mut self) -> &mut T {
		&mut self.val
	}
}

/// Reference to the entry in the rerence list.
#[derive(Debug)]
pub struct EntryRef<T>(Rc<RefCell<Entry<T>>>);

impl<T> Clone for EntryRef<T> {
	fn clone(&self) -> Self {
		EntryRef(self.0.clone())
	}
}

impl<T> From<Entry<T>> for EntryRef<T> {
	fn from(v: Entry<T>) -> Self {
		EntryRef(Rc::new(RefCell::new(v)))
	}
}

impl<T> EntryRef<T> {
	/// Read the reference data.
	pub fn read(&self) -> ::std::cell::Ref<Entry<T>> {
		self.0.borrow()
	}

	/// Try to modify internal content of the referenced object.
	///
	/// May panic if it is already borrowed.
	pub fn write(&self) -> ::std::cell::RefMut<Entry<T>> {
		self.0.borrow_mut()
	}

	/// Index of the element within the reference list.
	pub fn order(&self) -> Option<usize> {
		self.0.borrow().order()
	}

	/// Number of active links to this entity.
	pub fn link_count(&self) -> usize {
		Rc::strong_count(&self.0) - 1
	}
}

/// List that tracks references and indices.
#[derive(Debug)]
pub struct RefList<T> {
	items: Vec<EntryRef<T>>,
}

impl<T> Default for RefList<T> {
	fn default() -> Self {
		RefList { items: Default::default() }
	}
}

impl<T> RefList<T> {

	/// New empty list.
	pub fn new() -> Self { Self::default() }

	/// Push new element in the list.
	///
	/// Returns refernce tracking entry.
	pub fn push(&mut self, t: T) -> EntryRef<T> {
		let idx = self.items.len();
		let val: EntryRef<_> = Entry::new(t, idx).into();
		self.items.push(val.clone());
		val
	}

	/// Start deleting.
	///
	/// Start deleting some entries in the list. Returns transaction
	/// that can be populated with number of removed entries.
	/// When transaction is finailized, all entries are deleted and
	/// internal indices of other entries are updated.
	pub fn begin_delete(&mut self) -> DeleteTransaction<T> {
		DeleteTransaction {
			list: self,
			deleted: Vec::new(),
		}
	}

	/// Get entry with index (checked).
	///
	/// Can return None when index out of bounts.
	pub fn get(&self, idx: usize) -> Option<EntryRef<T>> {
		self.items.get(idx).cloned()
	}

	fn done_delete(&mut self, indices: &[usize]) {
		for idx in indices {
			let mut detached = self.items.remove(*idx);
			detached.write().index = EntryOrigin::Detached;
		}

		for index in 0..self.items.len() {
			let mut next_entry = self.items.get_mut(index).expect("Checked above; qed").write();
			let total_less = indices.iter()
				.take_while(|x| **x < next_entry.order().expect("Items in the list always have order; qed"))
				.count();
			match next_entry.index {
				EntryOrigin::Detached => unreachable!("Items in the list always have order!"),
				EntryOrigin::Index(ref mut idx) => { *idx -= total_less; },
			};
		}
	}

	/// Delete several items.
	pub fn delete(&mut self, indices: &[usize]) {
		self.done_delete(indices)
	}

	/// Delete one item.
	pub fn delete_one(&mut self, index: usize) {
		self.done_delete(&[index])
	}

	/// Initialize from slice.
	///
	/// Slice members are cloned.
	pub fn from_slice(list: &[T]) -> Self
		where T: Clone
	{
		let mut res = Self::new();

		for t in list {
			res.push(t.clone());
		}

		res
	}

	/// Length of the list.
	pub fn len(&self) -> usize {
		self.items.len()
	}

	/// Clone entry (reference counting object to item) by index.
	///
	/// Will panic if index out of bounds.
	pub fn clone_ref(&self, idx: usize) -> EntryRef<T> {
		self.items[idx].clone()
	}

	/// Get reference to entry by index.
	///
	/// Will panic if index out of bounds.
	pub fn get_ref(&self, idx: usize) -> &EntryRef<T> {
		&self.items[idx]
	}

	/// Iterate through entries.
	pub fn iter(&self) -> slice::Iter<EntryRef<T>> {
		self.items.iter()
	}
}

/// Delete transaction.
#[must_use]
pub struct DeleteTransaction<'a, T> {
	list: &'a mut RefList<T>,
	deleted: Vec<usize>,
}

impl<'a, T> DeleteTransaction<'a, T> {
	/// Add new element to the delete list.
	pub fn push(self, idx: usize) -> Self {
		let mut tx = self;
		tx.deleted.push(idx);
		tx
	}

	/// Commit transaction.
	pub fn done(self) {
		let indices = self.deleted;
		let list = self.list;
		list.done_delete(&indices[..]);
	}
}

#[cfg(test)]
mod tests {

	use super::*;

	#[test]
	fn order() {
		let mut list = RefList::<u32>::new();
		let item10 = list.push(10);
		let item20 = list.push(20);
		let item30 = list.push(30);

		assert_eq!(item10.order(), Some(0usize));
		assert_eq!(item20.order(), Some(1));
		assert_eq!(item30.order(), Some(2));

		assert_eq!(**item10.read(), 10);
		assert_eq!(**item20.read(), 20);
		assert_eq!(**item30.read(), 30);
	}

	#[test]
	fn delete() {
		let mut list = RefList::<u32>::new();
		let item10 = list.push(10);
		let item20 = list.push(20);
		let item30 = list.push(30);

		list.begin_delete().push(1).done();

		assert_eq!(item10.order(), Some(0));
		assert_eq!(item30.order(), Some(1));
		assert_eq!(item20.order(), None);
	}
}