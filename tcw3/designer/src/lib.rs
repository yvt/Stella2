//! Declarative UI for TCW3
//!
//! Most parts of UI are static and imperative programming is not the best
//! option to write such things as it leads to an excessive amount of
//! boilerplate code. TCW3 Designer is a code generation framework that
//! addresses this issue.
//!
//! TCW3 designer is designed to meet the following requirements:
//!
//! - The structures of UI components can be expressed in a way that is mostly
//!   free of boilerplate code for procedurally constructing a structure.
//! - It generates widget controller types akin to standard widgets such as
//!   `tcw3::ui::views::Button` and they can be used in a similar way.
//! - Components in one crate can consume other components from another crate.
//! - Seamlessly integrates with existing components.
//!
//! # Usage
//!
//! TODO - please see `tcw3_meta`.
//!
//! # Language Reference
//!
//! TODO
//!
//! ## Fields: `const name: u32`, etc.
//!
//! Fields are values stored in each instance of a component. There are three
//! types of fields:
//!
//! - **`const`**: Consts are initialized once when the component is created.
//! - **`prop`**: Props are initialized once when the component is created, and
//!   can be mutated through a setter method.
//! - **`wire`**: Wires are reactive values and automatically (re-)evaluated
//!   using a dynamic expression.
//!
//! **Accessors:**
//!
//! ```text
//! prop prop1: u32; // Default accessor set
//! prop prop2: u32 { set; pub get; } // Read/write prop, but the setter is private
//! const const1: u32 { pub set; } // Initialized through the builder type, but
//!                                // can never be read
//! const const2: u32 {} = || 42; // Initialized as a fixed value 42. Cannot be
//!                               // overridden through the builder type. And
//!                               // can never be read.
//! const const3: u32 { pub set; pub get clone; }
//!                                // This is a customizable constant value.
//!                                // The getter returns a cloned value on
//!                                // contrary to the default behavior of `const`.
//! wire wire1: u32 { pub get; pub watch event (event1); } = || expr...;
//!                                // `event1` is raised whenever the value
//!                                // changes
//! ```
//!
//! Fields have zero or more accessors:
//!
//! - **`get`** creates a getter method. There are two kinds of getter methods:
//!     - `get clone` generates a getter method like `fn fieldname(&self) -> T`
//!       that returns a cloned value.
//!     - `get borrow` generates a getter method like
//!       `fn fieldname(&self) -> impl Deref<Target = T> + '_` that returns a
//!       borrowed value. For `const`, it returns a reference. For other kinds
//!       of fields, it returns a smart pointer (this is an implementation
//!       detail that shouldn't matter for most cases).
//!     - `get` chooses a default kind based on the default accessor set for
//!       the field.
//!
//! - (all except `wire`) **`set`** creates setter methods in two places: (1)
//!   `fn with_fieldname(self, new_value: T)` for the component's builder type.
//!   (2, `prop` only) `fn set_fieldname(&self, new_value: T)` for the compoent
//!   type. Note that the latter method does not instantly update the value.
//!   Please see the section *Updating State*.
//!
//! - (all except `const`) **`watch`** associates the field with an event.
//!     - The event is raised whenever the field is updated with a new value
//!       (more precisely, after *a commit operation* that updated the field
//!       with a value that is not equal to the old value). The event must have
//!       no parameters.
//!     - `watch` makes it possible for other components to use the field's
//!       value through a dynamic expression.
//!
//! When accessors are omitted, the default accessor set for the field type
//! (shown in the table below) is used. The accessors will have the field's
//! visibility specifier. (Actually, this is the only case where the field's
//! visibility specifier matters.)
//!
//! | Field Type | `get`        | `set` | `watch` |
//! | ---------- | ------------ | ----- | ------- |
//! | `const`    | `get borrow` |       | N/A     |
//! | `prop`     | `get clone`  | `set` |         |
//! | `wire`     | `get clone`  | N/A   |         |
//!
//! (N/A: not applicable for the field type)
//!
//! > **Rationale:** The reason the kinds of `get` differ between `const` and
//! > other kinds of fields is that the latter kinds of fields are stored behind
//! > `RefCell` and require a runtime check to borrow safely.
//!
//! **Values:**
//! The optionally-specified dynamic expression in the right-hand side of `=` is
//! *a default value* (`const`, `prop`) or *a reactive value* (`wire`).
//!
//! ```text
//! prop prop1: u32 = || 42; // Defaults to 42 but can be changed later or
//!                       // when constructing the component
//! const const1: u32 = || 42; // 42 all the time
//!
//! // RHS of `wire` is a reactive value
//! wire wire1: u32 = |prop1| prop1 + 1; // Automatically updated based on the
//!                                      // value of `prop1`
//! ```
//!
//! If the value is omitted, the field must be explicitly initialized through
//! a setter method of the builder type. If the field doesn't have a setter
//! either, the field will be impossible to initialize, which is illegal:
//!
//! ```text,should_error
//! const const1: u32 = || 42;            // ok: initialized to 42
//! const const2: u32 { pub(crate) set; } // ok: the builder provides a value
//! const const3: u32;                    // ERROR
//!
//! prop prop1: u32;                      // ok: the builder provides a value
//! prop prop2: u32 {}                    // ERROR
//! ```
//!
//! ## Dynamic Expressions: `|prop1| prop1 + 1`, etc.
//!
//! Fields (`const`, `prop`, and `wire`) and event handlers (`on`) may have *a
//! dynamic expression* that is evaluated to compute their value or take a
//! responsive action.
//!
//! Dynamic expressions are *not* Rust expression by themselves. They take one
//! of the following forms:
//!
//! **Function: `|funcinputs...| rust-expr`** —
//! `rust-expr` is a Rust expression that is inserted verbatim into the
//! generated implementation code. `funcinputs...` is zero or more *inputs* to
//! the expression with optional decorations shown below:
//!
//!  - **`&input`** — Takes a reference to the value. This is preferred to the
//!    default (by-value) mode because it avoids potentially expensive cloning.
//!  - **`input as altname`** — Reads the input under a different name.
//!
//! Here are some examples (assuming `ComponentName` is the enclosing component):
//!
//!  - `&this` imports `&ComponentName` representing the current component
//!    as `this`
//!  - `field.text as x` reads the value of `prop text` of `const field` of
//!    the current component, clones it, and makes it available as a variable
//!    named `x`.
//!  - `field.event` (`event` refers to an event) does not load any value.
//!    Instead, it instructs the system to re-evaluate the value when the event
//!    is raised. **Warning:** In an `on` item, this must be specified in the
//!    trigger part (between `(...)`). It has no effect in the handler part
//!    (between `|...|`).
//!  - `init` is similar to the last one, but it's triggered after the enclosing
//!    component is instantiated.
//!  - `event.wm` gets the value of an event parameter named `wm`.
//!
//! ```tcwdl,no_compile
//! // `displayed_text` is re-evaluated whenever `count` changes
//! wire displayed_text: String = |count|
//!     format!("You pressed this button for {} time(s)!", count);
//!
//! // `max_height` is calculated based on `vertical`. `max_height` is `const`,
//! // so `vertical` must also be `const`.
//! const default_max_height: f32 = |vertical|
//!     if vertical { 1.0 / 0.0 } else { 32.0 };
//!
//! // The expression after `=` represents the default value of `max_height`.
//! // `default_max_height` must be `const` because a default value can't change
//! // over time.
//! prop max_height: f32 = |default_max_height| default_max_height;
//!
//! // An event handler that runs on various occassions.
//! on (init, button1.activated, displayed_text)
//!     |displayed_text| { println!("text = {}", displayed_text); }
//!
//! // An event handler receiving a parameter.
//! on (dnd_receiver.drop_file) |event.file| { dbg!(&file); }
//! ```
//!
//! The expression being inserted verbatim means it can access items defined in
//! the same scope where `designer_impl!` is used.
//!
//! **Object initialization literal: `ComponentName { prop foo = ...; ... }`**
//! Instantiates the component named `ComponentName` *exactly once* when the
//! current component is created. The component's fields are initialized with
//! specified dynamic expressions and kept up-to-date by re-evaluating the
//! expressions as needed in a way similar to `const` and `wire`.
//!
//! ```tcwdl,no_compile
//! const button = Button {
//!     const style_manager = |style_manager| style_manager;
//!     prop caption = |this.count|
//!         format!("You pressed this button for {} time(s)!", count);
//! };
//! ```
//!
//! ## Inputs
//!
//! *Inputs* (e.g., `this.prop` in `wire foo = |&this.prop| *prop + 42`)
//! represent a value used as an input to calculation as well as specifying
//! the trigger of an event handler. They are defined recursively as follows:
//!
//!  - `ϕ` is de-sugared into `this.ϕ` if it does not start with `this.` or
//!    `event.`.
//!  - `this` is an input.
//!  - `this.item` is an input if the enclosing component (the surrounding
//!    `comp` block) has a field or event named `field`.
//!  - If `ϕ` is an input representing a `const`¹ field, the field
//!    stores a component, and the said component has a field or event named
//!    `item`, then `ϕ.item` is an input.
//!  - `event.param` is an input if the input is specified in the handler
//!    function of an `on` item (i.e., in `on (x) |y| { ... }`, `y` meets this
//!    condition but `x` does not), the trigger input (i.e., `x` in the previous
//!    example) only contains inputs representing one or more events, and all of
//!    the said events have a parameter named `param`.
//!
//! ¹ This restriction may be lifted in the future.
//!
//! Inputs appear in various positions with varying roles, which impose
//! restrictions on the kinds of the inputs' referents:
//!
//! | Position              | Role     |
//! | --------------------- | -------- |
//! | `on` trigger          | Trigger  |
//! | `on` handler function | Sampled  |
//! | `const`               | Static   |
//! | `prop`                | Static   |
//! | `wire`                | Reactive |
//! | obj-init → `const`    | Static   |
//! | obj-init → `prop`     | Reactive |
//!
//! - If the role is **Reactive** or **Trigger**, the input must be watchable.
//!   That is, the referent must be one of the following:
//!     - A `const` field.
//!     - A `prop` or `wire` field in a component other than the enclosing
//!       component, having a `watch` accessor visible to the enclosing
//!       component.
//!     - Any field of the enclosing component.
//!     - An `event` item.
//! - If the role is **Static**, the referent must be a `const` field.
//!
//! ## Component Attributes
//!
//!  - **`#[prototype_only]`** suppresses the generation of implementation code.
//!  - **`#[widget]`** indicates that the component is a widget controller type.
//!    The precise semantics is yet to be defined and this attribute does
//!    nothing at the moment.
//!  - **`#[builder(simple)]`** changes the builder API to the simple builder
//!    API often used by standard widgets. Because Designer does not support
//!    generating the code generation for the simple builder API,
//!    **`#[prototype_only]` must also be specified**.
//!
//! ## Lifetime Elision
//!
//! Fields have implicit `'static` lifetimes like constant and static
//! declarations in Rust.
//!
//! ```text
//! // prop1: &'static str
//! prop prop1: &str;
//! ```
//!
//! Due to the code generator's lack of access to Rust's type system, it can't
//! deduce lifetimes for implicit lifetime parameters (this is unidiomatic in
//! Rust 2018). They will cause a compilation error when the generated code is
//! compiled.
//!
//! ```text
//! // ok: std::borrow::Cow<'static, str>
//! prop ok: std::borrow::Cow<'_, str>;
//!
//! // bad: Designer doesn't report any errors, but this will not compile
//! prop bad: std::borrow::Cow<str>;
//! ```
//!
//! ## Doc comments
//!
//! Components, fields, and events can have doc comments. They work in the
//! same way as in Rust.
//!
//! ```text
//! /** `MyComponent`'s description */
//! comp MyComponent {
//!     /// `prop1`'s description
//!     prop prop1: u32 = || 42;
//! }
//! ```
//!
//! ## Limiations
//!
//! - The code generator does not have access to Rust's full type system.
//!   Therefore, it does not perform type chacking at all.
//!
//! # Details
//!
//! ## Crate Metadata
//!
//! ```text
//! ,-> tcw3 -> tcw3_designer_runtime                    tcw3_designer <-,
//! |                                                                    |
//! |    ,----------,  dep   ,---------------,  codegen  ,----------,    |
//! | <- | upstream | -----> | upstream_meta | <-------- | build.rs | -> |
//! |    '----------'        '---------------'           '----------'    |
//! |         ^                      ^                         build-dep |
//! |         |                      |       build-dep                   |
//! |         | dep                  '------------------------,          |
//! |         |                                               |          |
//! |         |                                               |          |
//! |    ,----------,  dep   ,---------------,  codegen  ,----------,    |
//! '--- | applicat | -----> | applicat_meta | <-------- | build.rs | ---'
//!      '----------'        '---------------'           '----------'
//! ```
//!
//! In order to enable the consumption of other crate's components, TCW3
//! Designer makes use of build scripts. Each widget library crate has a meta
//! crate indicated by the suffix `_meta`. The source code of each meta crate
//! is generated by the build script, which can access other crates' information
//! by importing their meta crates through `build-dependencies`.
//!
//! ## Meta Crates
//!
//! Meta crates include a build script that uses [`BuildScriptConfig`] to
//! generate the source code of the crate. The generated code exports the
//! following two items:
//!
//! ```rust,no_compile
//! pub static DESIGNER_METADATA: &[u8] = [ /* ... */ ];
//! #[macro_export] macro_rules! designer_impl { /* ... */ }
//! ```
//!
//! `DESIGNER_METADATA` is encoded metadata, describing components and their
//! interfaces provided by the crate. You call [`BuildScriptConfig::link`] to
//! import `DESIGNER_METADATA` from another crate.
//!
//! `designer_impl` is used by the main crate to generate the skeleton
//! implementation for the defined components.
//!
//! ## Component Types
//!
//! For a `pub` component named `Component`, the following four types are
//! defined (they are inserted to a source file by `designer_impl` macro):
//!
//! ```rust,no_compile
//! pub struct ComponentBuilder<T_mandatory_attr> {
//!     mandatory_attr: T_mandatory_attr,
//!     optional_attr: Option<u32>,
//! }
//!
//! pub struct Component {
//!     shared: Rc<ComponentShared>,
//! }
//!
//! struct ComponentShared {
//!     state: RefCell<ComponentState>,
//!     dirty: Cell<u8>,
//!     subs: [std::mem::MaybeUninit<subscription_list::Sub>; 10],
//!     value_prop1: Cell<Option<u32>>, // uncommited value
//!     value_const1: u32,
//!     subscriptions_event1: RefCell<_>,
//!     /* ... */
//! }
//!
//! struct ComponentState {
//!     value_prop1: u32,
//!     value_wire1: u32,
//!     /* ...*/
//! }
//! ```
//!
//! ## Builder Types
//!
//! `ComponentBuilder` implements a moving builder pattern (where methods take
//! `Self` and return a modified instance, destroying the original one). It
//! uses a generics-based trick to ensure that the mandatory parameters are
//! properly set at compile-time.
//!
//! ```rust,no_compile
//! use tcw3::designer_runtime::Unset;
//!
//! pub struct ComponentBuilder<T_mandatory_attr> {
//!     mandatory_attr: T_mandatory_attr,
//! }
//!
//! // `Unset` represents those "holes"
//! impl ComponentBuilder<Unset> { pub fn new() -> Self { /* ... */ } }
//!
//! // `build` appears only if these holes are filled
//! impl ComponentBuilder<u32> { pub fn build() -> Component { /* ... */ } }
//! ```
//!
//! Components with `#[builder(simple)]` use *the simple builder API*.
//! The simple builder API does not provide a builder type and instead the
//! component is instantiated by its method `new` that accepts initial field
//! values in the order defined in the component. Optional `const` fields
//! are not allowed to have a setter method because there's no way to set
//! them. This means that every `const` field either (1) has no default value
//! and must be specified through `new` or (2) has a default value that can't
//! be changed from outside.
//!
//! ```rust,no_compile
//! // Standard builder
//! ScrollbarBuilder::new().vertical(true).build()
//! // Simple builder
//! Scrollbar::new(true).build()
//! ```
//!
//! The reason to support this builder API is to facilitate the integration
//! with hand-crafted components since the simple builder API is easier to
//! write manually.
//!
//! ## Component Initialization
//!
//! **Field Initialization** —
//! The first step in the component constructor `Component::new` is to evaluate
//! the initial values of all fields and construct `ComponentState`,
//! `ComponentShared`, and then finally `Component`.
//!
//! A dependency graph is constructed. Each node represents one of the
//! following: (1) A field having a value, which is either an object
//! initialization literal `OtherComp { ... }` or a function `|dep| expr`.
//! (2) A `const` or `prop` field in an object initialization literal in
//! `Component`.
//! A topological order is found and the values are evaluated according to that.
//! Note that because none of the component's structs are available at this
//! point, **`this` cannot be used as an input to any of the fields** involved
//! here. Obviously, fields that are not initialized at this point cannot be
//! used as an input.
//!
//! **Events** —
//! Event handlers are hooked up to child objects. The following table
//! summarizes how each combination of a trigger type and its context is
//! handled:
//!
//! | Position          | Input              | Mode                |
//! | ----------------- | ------------------ | ------------------- |
//! | `on` trigger      | `this.event`       | Direct              |
//! | ↑                 | `this.field.event` | ↑                   |
//! | `on` trigger      | `this.field`       | Dirty Flag Internal |
//! | `wire`            | ↑                  | ↑                   |
//! | obj-init → `prop` | ↑                  | ↑                   |
//! | `on` trigger      | `this.field.field` | Dirty Flag External |
//! | `wire`            | ↑                  | ↑                   |
//! | obj-init → `prop` | ↑                  | ↑                   |
//! | `on` trigger      | `this.field.event` | Dirty Flag External |
//! | `wire`            | ↑                  | ↑                   |
//! | obj-init → `prop` | ↑                  | ↑                   |
//!
//!  - If the mode is **Direct**, the given Rust expression is directly
//!    registered as the event handler. This makes it possible for the
//!    expression to access the event parameters, which might not outlive
//!    the duration of the handler function call.
//!  - If the mode is **Dirty Flag**, a dirty flag is created to indicate
//!    whether the given expression should be evaluated on an upcoming commit
//!    operation. **Internal** and **External** specifies the possible pathway
//!    through which the dirty flag is set.
//!      - **Internal** means the dirty flag is set in response to a change in
//!        the same component's another field, e.g., by a `prop`'s setter or
//!        a `wire`'s recalculation.
//!      - **External** means an event handler is registered and the dirty flag
//!        is set by the handler.
//!
//! **Direct** and **Dirty Flag External** modes are implemented by calling the
//! subscription function of the observed event, which returns
//! `tcw3::designer_runtime::Sub`.
//! They are automatically unsubscribed when `ComponentShared` is dropped. The
//! way this is implemented in (see *Component Destruction*) requires an access
//! to `Wm`, so the component **must have a `const` field named `wm`** if it has
//! anything handled in any of these modes.
//!
//! Event handlers maintain weak references to `ComponentShared`.
//!
//! ## Updating State
//!
//! After dependencies are updated, recalculation (called *a commit operation*)
//! of props and wires is scheduled using `tcw3::uicore::WmExt::invoke_on_update`.
//! Since it's possible to borrow the values of props and wires anytime, the
//! callback function of `invoke_on_update` is the only place where the values
//! can be mutated reliably (though this is not guaranteed, so runtime checks
//! are still necessary for safety).
//! Most importantly, even the effect of prop setters are deferred in this way.
//! New prop values are stored in a separate location until they are assigned
//! during a commit operation.
//!
//! An access to `Wm` is needed to call `invoke_on_update`. Therefore, the
//! component **must have a `const` field named `wm`** for the process described
//! here to happen. The type of `wm` is not checked (because Designer doesn't
//! have access to Rust's type system), but it must be `tcw3::pal::Wm`.
//!
//! ```tcwdl
//! comp MyComponent {
//!     const wm: tcw::pal::Wm { pub set; }
//!
//!     // Props are updated through the reactive update mechanism, so this
//!     // component must have `wm` field.
//!     pub prop prop1: u32 = || 42;
//! }
//! ```
//!
//! A bit array is used as dirty flags for tracking which fields need to be
//! recalculated. Basically, each obj-init prop and wire with a functional value
//! receives a dirty flag. In addition, each event handler watching a field also
//! receives a dirty flag (see *Component Initialization* for more).
//!
//! ```tcwdl
//! // `foo`'s setter sets the dirty flags for `bar1` and `bar2`.
//! prop foo: u32;
//! wire bar1: u32 = |foo| foo + 1;
//! wire bar2: u32 = |foo| foo + 2;
//! // After the new value of `bar1` is calculated and it's different from the
//! // old value, `hoge`'s dirty flag is set and the new value of `hoge` is
//! // calculated in turn.
//! wire hoge: u32 = |bar1| bar1 * 2;
//! ```
//!
//! The dirty flags are sorted in the evaluation order.
//!
//! In order to optimize the usage of dirty flags, a group of flags which are
//! set at the same time is combined into a single bit. The optimized flags are
//! called *compressed dirty flags*. Each compressed dirty flag corresponds to
//! zero or more raw dirty flags.
//!
//! The generated commiting function looks like the following:
//!
//! ```rust,no_compile
//! use std::{hint::unreachable_unchecked, mem::forget};
//! use harmony::ShallowEq;
//! fn commit(&self) {
//!     let shared = &*self.shared;
//!     let dirty = shared.dirty.replace(0);
//!
//!     // Uncommited props must be read first because we reset `self.dirty`
//!     // at the same time. Otherwise, `uncommited_foo` gets leaked on panic
//!     let new_foo = if (dirty & 1 != 0) {
//!         Some(shared.uncommited_foo.replace(MaybeUninit::uninit()).read())
//!     } else {
//!         None
//!     };
//!
//!     let state = shared.state.borrow();
//!     let mut foo = &state.foo;
//!     let mut bar1 = &state.bar1;
//!     let mut bar2 = &state.bar2;
//!     let mut hoge = &state.hoge;
//!     if (dirty & (1 << 0)) != 0 {
//!         // Commit `prop`
//!         let new_foo_t = new_foo.as_ref().unwrap_or_else(unsafe { unreachable_unchecked () });
//!         if foo != new_foo_t {
//!             dirty |= 1 << 1;
//!         }
//!         foo = new_foo_t;
//!     }
//!     let new_bar1;
//!     let new_bar2;
//!     if (dirty & (1 << 1)) != 0 {
//!         // Commit `bar1` and `bar2`
//!         let new_bar1_t = *foo + 1;
//!         if !ShallowEq::shallow_eq(bar1, &new_bar1_t) {
//!             dirty |= 1 << 2;
//!         }
//!         new_bar1 = Some(new_bar1_t);
//!         bar1 = new_bar1.as_ref().unwrap();
//!
//!         let new_bar2_t = *foo + 2;
//!         new_bar2 = Some(new_bar2_t);
//!         bar2 = new_bar2.as_ref().unwrap();
//!     } else {
//!         new_bar1 = None;
//!         new_bar2 = None;
//!     }
//!     let new_hoge;
//!     if (dirty & (1 << 2)) != 0 {
//!         // Commit `hoge`
//!         let new_hoge_t = *bar1 * 2;
//!         new_hoge = Some(new_hoge_t);
//!         hoge = new_hoge.as_ref().unwrap();
//!     } else {
//!         new_hoge = None;
//!     }
//!     drop(state);
//!
//!     // Write back
//!     let mut state = shared.state.borrow_mut();
//!     if (dirty & (1 << 0)) != 0 {
//!         state.foo = new_foo.unwrap_or_else(unsafe { unreachable_unchecked () });
//!     } else {
//!         forget(new_foo);
//!     }
//!     if (dirty & (1 << 1)) != 0 {
//!         state.bar1 = new_bar1.unwrap_or_else(unsafe { unreachable_unchecked () });
//!         state.bar2 = new_bar2.unwrap_or_else(unsafe { unreachable_unchecked () });
//!     } else {
//!         forget(new_bar1);
//!         forget(new_bar2);
//!     }
//!     if (dirty & (1 << 2)) != 0 {
//!         state.hoge = new_hoge.unwrap_or_else(unsafe { unreachable_unchecked () });
//!     } else {
//!         forget(new_hoge);
//!     }
//!     drop(state);
//!
//!     if (dirty & (1 << 1)) != 0 {
//!         self.raise_hoge_changed();
//!     }
//! }
//! ```
//!
//! ## Component Destruction
//!
//! In the `Drop` implementation of `ComponentShared`, event handlers are
//! unregistered from their respective events. This is done by calling
//! `subscriber_list::Sub::unsubscribe()`, but the caveat is that this method
//! fails if it is called from the same event's handler. For this reason, the
//! `Drop` implementation just enqueues a closure using `Wm::invoke`, and this
//! closure unregisters the event handlers.
//!
//! # Tests
//!
//! This crate includes two categories of tests:
//!
//!  - `tests/bad`: A set of TCWDL source files expected to be rejected by the
//!    code generator.
//!  - `tests_impl`: Processes TCWDL source files and validates the behavior of
//!    the generated code. Relies on `testing` backend.
//!
//! To run all tests for Designer, do the following:
//!
//! ```shell
//! cargo test -p tcw3_designer -p tcw3_designer_tests_impl --all-features
//! ```
mod codegen;
mod metadata;

pub use self::codegen::BuildScriptConfig;
