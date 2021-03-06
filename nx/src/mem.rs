extern crate alloc;

use linked_list_allocator::LockedHeap;
use alloc::rc;
use core::cell;

pub type SharedObject<T> = rc::Rc<cell::RefCell<T>>;

pub fn make_shared<T>(t: T) -> SharedObject<T> {
    SharedObject::new(cell::RefCell::new(t))
}

// TODO: switch from the spin crate this crate uses to our lock system

#[global_allocator]
static GLOBAL_ALLOCATOR: LockedHeap = LockedHeap::empty();

pub fn initialize(heap_address: *mut u8, heap_size: usize) {
    unsafe {
        GLOBAL_ALLOCATOR.lock().init(heap_address as usize, heap_size);
    }
}

#[alloc_error_handler]
fn alloc_error_handler(layout: core::alloc::Layout) -> ! {
    panic!("Memory allocation failed - size: {}, alignment: {}", layout.size(), layout.align())
}