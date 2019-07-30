//! Implements the table view, a scrollable widget that displays subviews in
//! imaginary table cells.
//!
//! # General Properties
//!
//! - The table view supports displaying > 1,000,000,000 lines (rows and
//!   columns).
//! - Lines can be dynamically inserted and/or removed. The time complexity of
//!   inserting/removing a single consecutive range of lines which are not in
//!   view is logarithmic in ordinary cases. (However, it's linear on regard to
//!   the number of visible lines.)
//! - Reordering is not natively supported; it must be implemented as removal
//!   followed by insertion at the new location.
//! - Lines have a homogeneous, dynamic size (width or height). To make this
//!   practical, the sizes of off-screen lines may use approximation. Their
//!   sizes are not affected in any way by subviews' size traits.
//!
//! ## Limitations
//!
//! - Line coordinates and line indices are represented using `i64`. Once the
//!   calculation result or intermediate results get past the representable
//!   range of `i64`, weird things might occur.
//!
//! # Concepts
//!
//! - A *line* refers to a column or a row in a table. When manipulating lines,
//!   [`LineTy`] specifies which of them is being updated. For example,
//!   `(LineTy::Row, 0)` represents the first row.
//! - The *line size* of a line is a width or height of the line.
//!
//! [`LineTy`]: table::LineTy
//!
//! ## Table model
//!
//! A table model is a conceptual entity representing the content of a table
//! view. It's comprised of two sequences of line sizes each for columns and
//! rows and subviews representing table cells, which are realized on-demand.
//!
//! Objects representing a table model are predominantly owned by `Table`.
//!
//!  - `Table` tracks the number of lines in the table model.
//!  - `Table` owns a [`TableModelQuery`] object supplied by the application.
//!     - `Table` gets line sizes or a sum of line sizes by calling a method of
//!        [`TableModelQuery`].
//!     - `Table` maintains a `HView` and [`CellCtrler`] for every cell in the
//!        view. They are created by calling a method of `TableModelQuery`.
//!
//! To start making changes to the table model, the application locks the table
//! model state by calling [`Table::edit`] and obtains a lock guard of type
//! [`TableEdit`]. The application makes changes by calling `TableEdit`'s
//! methods. Some editing operations require the application to follow a
//! particular sequence. For example, when inserting lines, the application must
//! first update `TableModelQuery` so that when `TableEdit::insert` looks at
//! it, all new lines are already inserted at the intended position.
//!
//! `TableEdit` provides a mutable reference to the current `TableModelQuery`
//! object. It's the only reasonable way to access the `TableModelQuery` object
//! owned by `Table` because mutating it in other ways is likely to
//! desynchronize the objects involved in shaping the table model.
//!
//! Editing operations supported by `TableEdit` are exposed through the trait
//! [`TableModelEdit`]. This allows the application to insert a model layer to
//! implement a functionality such as animation by designing the update code to
//! operate on generic `TableModelEdit` types, not just `TableEdit`.
//!
//! [`Table::edit`]: table::Table::edit
//! [`CellCtrler`]: table::CellCtrler
//! [`TableModelQuery`]: table::TableModelQuery
//! [`TableModelEdit`]: table::TableModelEdit
//! [`TableEdit`]: table::TableEdit
//!
use as_any::AsAny;
use bitflags::bitflags;
use cgmath::{Point2, Vector2};
use ndarray::Array2;
use std::{
    any::Any,
    cell::{Cell, RefCell},
    fmt,
    ops::Range,
    rc::Rc,
};

use crate::ui::scrolling::{
    lineset::{Index, Lineset, Size},
    tableremap::LineIdxMap,
};
use crate::uicore::{HView, SizeTraits, ViewFlags};

/// A scrollable widget displaying subviews along imaginary table cells.
///
/// See [the module documentation](index.html) for more.
#[derive(Debug)]
pub struct Table {
    view: HView,
    inner: Rc<Inner>,
}

/// A 0-based two-dimensional index `[row, column]` (or `[y, x]`) specifying a
/// single cell in a table.
pub type CellIdx = [u64; 2];

/// The underlying data of a table view.
#[derive(Debug)]
struct Inner {
    state: RefCell<State>,

    /// The widget size.
    ///
    /// Basically, `uicore` only tells the actual size to `Layout`. But to
    /// determine which lines are visible, we need to know the size.
    /// The solution is to store the last known size in `size`. When the
    /// `Layout::arrange` is called, it compares the actual size against the
    /// size stored in `size`. If they don't match, the curreent `Layout` is
    /// no longer valid. Thus, `size` is updated with the up-to-date value,
    /// new visible line sets are calculated, and a brand new `Layout` is
    /// created.
    ///
    /// The size is represented in the fixed-point format.
    size: Cell<Vector2<Size>>,

    /// The size traits of the table view.
    size_traits: Cell<SizeTraits>,

    /// The line coordinates of the respective top/left edge of the viewport.
    vp: [Cell<Size>; 2],

    dirty: Cell<DirtyFlags>,
}

// TODO: scrolling

/// A part of table view state data that is contained by `RefCell`.
/// The remaining part is in `Inner`, on the contrary to what the name implies.
struct State {
    model_query: Box<dyn TableModelQuery>,
    cells: Array2<TableCell>,

    /// Ranges of lines represented by `cells`.
    ///
    /// `cells` represents a rectangular region in the line index space. Each
    /// element in `cells_ranges` represents a range of lines for the
    /// corresponding (see `LineTy`) axis. E.g., `[0..4, 3..7]` means `cells`
    /// has cells from the 0–3rd rows.
    cells_ranges: [Range<Index>; 2],

    /// Used during remapping (the change of the range represented by `cells`).
    /// Logically it only lives during each run of remapping, but is stored
    /// as a part of `State` for optimization.
    ///
    /// The indices correspond to `LineTy`'s integer values.
    line_idx_maps: [LineIdxMap; 2],

    /// Stores line coordinates of line groups (one or more consecutive lines).
    ///
    /// The indices correspond to `LineTy`'s integer values.
    linesets: [Lineset; 2],
}

impl fmt::Debug for State {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("State")
            .field("model_query", &((&*self.model_query) as *const _))
            .field("cells", &self.cells)
            .field("line_idx_maps", &self.line_idx_maps)
            .field("linesets", &self.linesets)
            .finish()
    }
}

bitflags! {
    struct DirtyFlags: u8 {
        /// Indicates that the new values for `cells` and `cells_ranges` must be
        /// calculated based on the up-to-date `linesets` and viewport.
        const CELLS = 1 << 0;

        /// Indicates that the layout of the view is out-dated and must be
        /// replaced with a new layout.
        const LAYOUT = 1 << 1;
    }
}

struct TableCell {
    view: HView,
    ctrler: Box<dyn CellCtrler>,
}

impl fmt::Debug for TableCell {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("TableCell")
            .field("view", &self.view)
            .field("ctrler", &((&*self.ctrler) as *const _))
            .finish()
    }
}

/// A trait for objects that allow the consumer of a table model (usually
/// [`Table`]) to query properties of the table model.
///
/// `Table` stores a supplied `TableModelQuery` internally and uses it to
/// update the view when the view is resized or scrolled.
///
/// This can be thought of as the "pull" part of the table model, complemented
/// by [`TableModelEdit`]. Notice that `TableModelQuery` does not have methods
/// for querying the number of lines. This is because they are inserted and
/// removed by method calls to `TableModelEdit`, and `TableModelQuery` only
/// provides methods for querying their sizes.
///
/// # Guidelines for implementors
///
/// - Size queries should respond without a delay such as the one caused by disk
///   I/O because the latency directly affects the user interface performance.
///   When the dataset is large, it's often the case that the real sizes can't
///   be measured without disk I/O. In such cases, exact queries should return
///   a placeholder or approximate size, and then when the real measurement
///   is ready, they should be updated with real sizes. The cells should display
///   a placeholder content until the real measurement is ready.
///
pub trait TableModelQuery: AsAny + Any {
    /// Create a subview for the specified table cell.
    fn new_view(&mut self, cell: CellIdx) -> (HView, Box<dyn CellCtrler>);

    /// Get the total size of the lines in the specified range. The result may
    /// be approximate if `approx` is `true`.
    ///
    /// If `approx` is `false`, `range.end - range.start` must be equal to `1`.
    fn range_size(&mut self, line_ty: LineTy, range: Range<u64>, approx: bool) -> f64;
}

/// The default implementation of `TableModelQuery` that produces sane default
/// values.
impl TableModelQuery for () {
    fn new_view(&mut self, _cell: CellIdx) -> (HView, Box<dyn CellCtrler>) {
        (HView::new(Default::default()), Box::new(()))
    }

    fn range_size(&mut self, _line_ty: LineTy, range: Range<u64>, _approx: bool) -> f64 {
        10.0 * (range.end - range.start) as f64
    }
}

/// A trait for controller objects (e.g., [`Button`]), each of which controls
/// the associated subview embedded in a table cell of [`Table`].
///
/// [`Button`]: crate::ui::views::Button
///
/// Controller objects implementing `CellCtrler` are returned by
/// [`TableModelQuery::new_view`]. `Table` makes sure that controller objects
/// live as long as their corresponding view objects (this is required by some
/// views to behave correctly due to uses of weak references).
///
/// There are a few implementations that save the implementor of
/// `TableModelQuery` defining a new type implementing `CellCtrler` in some
/// situations: `()` is a no-op implementation, and `(T,)` wraps an arbitrary
/// existing type, only ensuring `T` lives long enough.
pub trait CellCtrler: 'static {
    // TODO: Notify the visible portion of the cell. This is useful when the
    //       cell is very large
}

impl CellCtrler for () {}
impl<T: 'static> CellCtrler for (T,) {}

/// A trait for making changes to a table model.
pub trait TableModelEdit {
    /// Get a mutable reference to the `TableModelQuery` object that the
    /// consumer of the table model uses to query properties of the model.
    ///
    /// The name `model` might be a bit of misnomer because a table model is not
    /// exactly realized as a single object (as often done in an
    /// object-oriented design). However, the natural design would be to store
    /// most of the implementation-specific state data in `TableModelQuery`
    /// because (1) the table view examines the model via `TableModelQuery` when
    /// the application does not have control, and (2) when inserting or
    /// removing lines, modification to the line size sequenences represented by
    /// `TableModelQuery` and method calls on `TableModelEdit` must be done in a
    /// particular order, so it would make sense to store the model state
    /// directly in `TableModelQuery`.
    ///
    /// `TableModelQuery` implements [`AsAny`], so `dyn TableModelQuery` can be
    /// downcasted to a concrete type.
    fn model_mut(&mut self) -> &mut dyn TableModelQuery;

    /// Set a new `TableModelQuery` object.
    ///
    /// See also: [`TableModelEditExt::set_model`].
    ///
    /// This simply replaces the current `TableModelQuery` object, thus calling
    /// this method alone does not modify or remove any lines associated with
    /// the old `TableModelQuery`. The following example shows how to replace
    /// all lines when switching between two table models:
    ///
    ///     # use tcw3::ui::views::table::*;
    ///     # fn test(
    ///     #    edit: &mut impl TableModelEdit, new_model: impl TableModelQuery,
    ///     #    old_model_rows: u64, new_model_rows: u64,
    ///     #    old_model_cols: u64, new_model_cols: u64,
    ///     # ) {
    ///     // Remove all rows and columns from the old model
    ///     edit.remove(LineTy::Row, 0..old_model_rows);
    ///     edit.remove(LineTy::Col, 0..old_model_cols);
    ///
    ///     // Swap the `TableModelQuery` object
    ///     edit.set_model(new_model);
    ///
    ///     // Insert all rows from the new model
    ///     edit.insert(LineTy::Row, 0..new_model_rows);
    ///     edit.insert(LineTy::Col, 0..new_model_cols);
    ///     # }
    ///
    fn set_model_boxed(&mut self, new_model: Box<dyn TableModelQuery>);

    /// State that zero or more lines were inserted at the specified range.
    fn insert(&mut self, line_ty: LineTy, range: Range<u64>);

    /// State that zero or more lines are going to be removed from the specified
    /// range.
    fn remove(&mut self, line_ty: LineTy, range: Range<u64>);

    /// State that zero or more lines in the specified range were resized.
    fn resize(&mut self, line_ty: LineTy, range: Range<u64>);

    /// Instruct to re-create subviews in the specified range of lines.
    fn renew_subviews(&mut self, line_ty: LineTy, range: Range<u64>);
}

/// An extension trait for [`TableModelEdit`].
pub trait TableModelEditExt: TableModelEdit {
    /// Set a new `TableModelQuery` object.
    ///
    /// This wraps the given object with `Box` and passes it to
    /// [`TableModelEdit::set_model_boxed`].
    fn set_model(&mut self, new_model: impl TableModelQuery) {
        self.set_model_boxed(Box::new(new_model))
    }

    /// Downcast the result of `self.model_mut()`.
    ///
    ///     # use tcw3::ui::views::table::*;
    ///     # fn test(
    ///     #    edit: &mut dyn TableModelEdit,
    ///     # ) {
    ///     # type MyModelQuery = Vec<f32>;
    ///     let my_model: &mut MyModelQuery = edit.model_downcast_mut()
    ///         .expect("wrong concrete type");
    ///     # }
    ///
    fn model_downcast_mut<T: Any>(&mut self) -> Option<&mut T> {
        (*self.model_mut()).as_any_mut().downcast_mut()
    }
}

impl<T: ?Sized + TableModelEdit> TableModelEditExt for T {}

/// Indicates failure to lock the table model state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EditLockError;

impl std::fmt::Display for EditLockError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(
            f,
            "the table model is currently being accessed by the owner"
        )
    }
}

impl std::error::Error for EditLockError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineTy {
    // TODO: Transpose the axes? They are confusing because they are the opposite
    //       of `Vector2`
    Row = 0,
    Col = 1,
}

impl LineTy {
    /// Convert `TypeTy` to an index. For example, it's used for indexing into
    /// `State::line_idx_maps`.
    fn i(self) -> usize {
        self as usize
    }

    /// Select and get a reference to an element of `Vector2` based on `LineTy`.
    fn vec_get<T>(self, v: &Vector2<T>) -> &T {
        &v[1 - self as usize]
    }

    /// Select and get a mutable reference to an element of `Point2` based on
    /// `LineTy`.
    fn point_get_mut<T>(self, v: &mut Point2<T>) -> &mut T {
        &mut v[1 - self as usize]
    }
}

impl Table {
    /// Construct a table view widget.
    pub fn new() -> Self {
        let inner = Inner {
            state: RefCell::new(State {
                model_query: Box::new(()),
                cells: Array2::from_shape_fn((0, 0), |_| unreachable!()),
                cells_ranges: [0..0, 0..0],
                line_idx_maps: [LineIdxMap::new(0..0), LineIdxMap::new(0..0)],
                linesets: [Lineset::new(), Lineset::new()],
            }),
            size: Cell::new(Vector2::new(0, 0)),
            size_traits: Cell::new(SizeTraits::default()),
            vp: [Cell::new(0), Cell::new(0)],
            dirty: Cell::new(DirtyFlags::empty()),
        };

        let inner = Rc::new(inner);

        let view = HView::new(ViewFlags::default() | ViewFlags::LAYER_GROUP);
        view.set_listener(listener::TableViewListener::new(Rc::clone(&inner)));
        view.set_layout(update::TableLayout::from_current_state(
            Rc::clone(&inner),
            &inner.state.borrow(),
        ));

        Self { view, inner }
    }

    /// Get a handle to the view representing the widget.
    pub fn view(&self) -> &HView {
        &self.view
    }

    /// Attempt to acquire a lock to update the table model.
    ///
    /// Locking fails if there is another agent accessing the table model. For
    /// example, this happens when methods of the registered `TableModelQuery`
    /// (which is one of the things that can be accessed through the lock)
    /// attempt to call this method.
    pub fn edit(&self) -> Result<TableEdit<'_>, EditLockError> {
        let state = self
            .inner
            .state
            .try_borrow_mut()
            .map_err(|_| EditLockError)?;

        Ok(TableEdit {
            view: &self.view,
            state,
            inner: &self.inner,
        })
    }

    /// Set new size traits.
    ///
    /// Must not have an active edit (the table model must be in the unlocked
    /// state).
    pub fn set_size_traits(&self, value: SizeTraits) {
        self.inner.size_traits.set(value);
        self.inner.set_dirty_flags(DirtyFlags::LAYOUT);
        Inner::update_layout_if_needed(&self.inner, &self.inner.state.borrow(), &self.view);
    }
}

mod edit;
mod fixedpoint;
mod listener;
mod update;

pub use self::edit::TableEdit;
