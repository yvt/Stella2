use freeze::{FreezableCell, FreezableCellFreezeError, FreezableCellLockError};

// Validate auto traits
#[allow(dead_code)]
static ASSERT_SYNC: FreezableCell<u32> = FreezableCell::new_unfrozen(42);
// TODO: Check `!Send` and `!Sync` somehow

#[test]
fn unfrozen() {
    let cell = FreezableCell::new_unfrozen(42);

    let mut lock = cell.unfrozen_borrow_mut().unwrap();
    assert_eq!(*lock, 42);
    *lock = 56;

    // Can't mutably borrow when it's already borrowed
    assert_eq!(
        cell.unfrozen_borrow_mut().err(),
        Some(FreezableCellLockError::Locked)
    );

    // Can't freeze when it's already borrowed
    assert_eq!(cell.freeze().err(), Some(FreezableCellFreezeError::Locked));

    drop(lock);

    // Can mutably borrow again after unborrowing
    let lock = cell.unfrozen_borrow_mut().unwrap();
    assert_eq!(*lock, 56);
}

#[test]
fn unfrozen_to_frozen() {
    let cell = FreezableCell::new_unfrozen(42);

    // Freeze the cell
    cell.freeze().unwrap();

    // Freezing it again is no-op
    cell.freeze().unwrap();

    // Locking methods for unfrozen cells should fail for frozen cells
    assert_eq!(
        cell.unfrozen_borrow_mut().err(),
        Some(FreezableCellLockError::Frozen)
    );

    let lock = cell.frozen_borrow().unwrap();
    assert_eq!(*lock, 42);

    // A frozen cell accepts multiple shared borrows.
    let lock = cell.frozen_borrow().unwrap();
    assert_eq!(*lock, 42);
}

#[test]
fn frozen() {
    let cell = FreezableCell::new_frozen(42);

    // Locking methods for unfrozen cells should fail for frozen cells
    assert_eq!(
        cell.unfrozen_borrow_mut().err(),
        Some(FreezableCellLockError::Frozen)
    );

    let lock = cell.frozen_borrow().unwrap();
    assert_eq!(*lock, 42);

    // A frozen cell accepts multiple shared borrows.
    let lock = cell.frozen_borrow().unwrap();
    assert_eq!(*lock, 42);
}
