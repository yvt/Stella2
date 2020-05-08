# TCW3 — Cross-platform GUI toolkit

<!-- This file is imported as the top-level doc comment of `tcw3` -->

## Architectural Overview

```text
             ┌────────────────────────┐┌──────────────────────────┐
           ┌─┤       Unit Tests       ││        Application       │
           │ └────────────────────────┘└───────────────────┬───┬──┘
  Simulate │ ╔═══════════════════╗ ┌────────────────────┐  │   │
    Events │ ║ tcw3::ui::views   ║ │   Custom Widgets   │  │   │
           │ ║      ╔════════════╝ └────────────┐       │  │ Initialization
         ┌─│─╢      ║ ╔═══════════════════════╗ │       │  │   │
Drawing, │ │ ║      ║ ║   tcw3::ui::theming   ║ │       │  │   │ Platform-
 layers, │ │ ╚══════╝ ╚═══════════════════════╝ └───────┘  │   │ specific
    etc. │ │ ╔═══════════════════════╗ ╔════════════════╗  │   │ operations
         │ │ ║     tcw3::uicore      ║ ║  tcw3::images  ║  │   │ (e.g., creating
         │ │ ╚═══════════════════════╝ ╚════════════════╝  ↓   │ a main menu on
         │ │ ╔═══════════════════════════════════════════════╗ │ Cocoa)
         └─│→║                  tcw3::pal                    ║ │
           │ ╟───────────────────────────────────────────────╢ │
           └→║             testing (optional)                ║ │
             ╟───────────────┬───────────────┬───────────┐   ║ │
   Backends: ║    windows    │     macos     │    gtk    │   ║ │
             ╚═══════════════╧═══════════════╧═══════════╧═══╝ ↓
             ┌────────────────────────────────────────────────────┐
             │     Win32, WinRT, Cocoa, GTK, GLib, Pango, ...     │
             └────────────────────────────────────────────────────┘
```

**[`tcw3::pal`]** abstracts the underlying window system and
graphics libraries. It defines the concept of main thread and a trait
[`Wm`] for compile-time thread checking. `Wm` also provides several entry
point functions for the clients to call. Several backends are provided.
During a build, one of them is chosen based on the target platform and given
feature flags, and its public types such as [`Wm` (type)] and [`HWnd`] are
reexported at the crate root. The clients are supposed to use these
reexported items.

[`tcw3::pal`]: tcw3_pal
[`Wm`]: tcw3_pal::iface::Wm
[`Wm` (type)]: tcw3_pal::Wm
[`HWnd`]: tcw3_pal::HWnd

The API surface of `tcw3::pal` is carefully designed so that all backends
can implement the same interface, while ensuring the features of each target
platform are well utilized, the abstraction overhead is reasonably low, and
the engineering cost is acceptable.
For example, details of widget handling are vastly different between
platforms, so `tcw3::pal` has no concept of widgets, and widgets are instead
realized by `tcw3::uicore`. On the other hand, with regard to text input
handling, text input widgets are modeled as an abstract text storage and
the widgets don't have to deal with key strokes entered by the user.

`tcw3::pal` has a headless backend named `testing`, which is used for
testing various components built based on `tcw3::pal`. `testing` has a
dedicated interface to programmatically control the simulated window system,
which is exposed at [`tcw3::pal::testing`].

[`tcw3::pal::testing`]: tcw3_pal::testing

**[`tcw3::images`]** provides a type **[`HImg`]**, which represents a
scale-independent image that is rasterized on-demand. `tcw3::ui::theming`
uses this extensively to render non-rectangular shapes in widgets.
`tcw3::images` also manages the list of DPI scaling factors currently in use
by the user's desktop. The cache of rasterized bitmaps are categorized by
DPI scaling factors, and bitmaps are automatically released when the
associated DPI scaling factor is no longer in use. To implement this cache
management policy, a signaling mechanism that notifies changes in the global
list of DPI scaling factors is needed, but `tcw3::pal` doesn't provide
a facility for that.
It's `tcw3::uicore` that tells `tcw3::images` which DPI scaling factor is in
use and which one is no longer used. Other clients may do the same, but this
is usually unnecessary.

[`tcw3::images`]: tcw3_images
[`HImg`]: tcw3_images::HImg

**[`tcw3::uicore`]** is a widget toolkit built on top of the aforementioned
subsystems. It introduces the concept of *views*, which are nestable
rectangular regions inside a window that can receive user inputs and display
graphical contents. It provides a framework for laying out views using
*layout* objects. It's responsible for routing input events received by
windows to appropriate views.

[`tcw3::uicore`]: crate::uicore

`uicore` is by no means meant to be a complete widget toolkit by itself. The
appearance of views is not defined at all by `uicore`. Also, `uicore`
doesn't completely hide `pal`, so TCW3 widgets and applications occasionally
have to use `pal` directly.

**[`tcw3::ui`]** is an assortment of libraries built on top of `uicore`.
**[`tcw3::ui::theming`]** is a styling framework that allows decoupling
between logic and style. `theming` provides a view [`StyledBox`], which is
often used to define the appearance of widgets.
**[`tcw3::ui::mixins`]** provides *mix-ins*, which handle input events
(through composition) to implement common GUI behaviors.
Other submodules provide useful premade views and layouts.

[`tcw3::ui`]: crate::ui
[`tcw3::ui::theming`]: crate::ui::theming
[`tcw3::ui::mixins`]: crate::ui::mixins
[`StyledBox`]: crate::ui::theming::StyledBox

**`tcw3_designer`** (Designer) is a declarative framework for writing
GUI components with less boilerplate code. It's meant to be invoked by a
build script (`build.rs`). The generate code requires a runtime library
located in [`tcw3::designer_runtime`].
`designer` stores the definition of components in *meta crates*. Build
scripts invoking `designer` should import the definition of components they
use from meta crates. For the widgets defined by `tcw3::ui`, the
meta crate is [`tcw3_meta`].

[`tcw3::designer_runtime`]: tcw3_designer_runtime

## Features

### Main Thread

TCW3 relies on the concept of main thread. A main thread is defined by the
possession of an instance of a non-`Send`-able marker type [`tcw3::pal::Wm`].

[`tcw3::pal::Wm`]: tcw3_pal::Wm

### Event Loop

A TCW3 application enters a main event loop by calling [`Wm::enter_main_loop`].
The event loop monitors for events sent by the target window system, processes
them, and calls application-provided event listeners (such as those in
[`tcw3::pal::iface::WndListener`]) as needed.

[`Wm::enter_main_loop`]: tcw3_pal::iface::Wm::enter_main_loop
[`tcw3::pal::iface::WndListener`]: tcw3_pal::iface::WndListener

You can use [`Wm::invoke`] and similar methods to have a custom closure called
inside the main event loop. The following list summarizes the methods in this
category:

 - `Wm::invoke` enqueues a closure to the event queue. This is a low-level
   method that `uicore` relies on, and application developers should prefer
   `uicore::WmExt::invoke_on_update` over this.

 - [`Wm::invoke_on_main_thread`] is similar to above, but can be called by any
   thread.

 - [`Wm::invoke_after`] enqueues a closure to be called after a given delay.

 - [`uicore::WmExt::invoke_on_update`] is similar to `Wm::invoke`, but ensures
   the closure is called before `uicore` updates window contents.

 - [`uicore::HWnd::invoke_on_next_frame`] is similar to `Wm::invoke`, but calls
   are synchronized to the refresh rate of the display where the given `HWnd`
   is currently located. You should use this method to schedule screen updates
   for animation.

[`Wm::invoke`]: tcw3_pal::iface::Wm::invoke
[`Wm::invoke_on_main_thread`]: tcw3_pal::iface::Wm::invoke_on_main_thread
[`Wm::invoke_after`]: tcw3_pal::iface::Wm::invoke_after
[`uicore::WmExt::invoke_on_update`]: crate::uicore::WmExt::invoke_on_update
[`uicore::HWnd::invoke_on_next_frame`]: crate::uicore::HWnd::invoke_on_next_frame

[`Wm::terminate`] instructs the system to stop the main event loop, process all
remaining events, and exit the application.

[`Wm::terminate`]: tcw3_pal::iface::Wm::terminate

### 2D Graphics

Follow these steps to create a bitmap:

 1. Use [`tcw3::pal::BitmapBuilder`]​[`::new`] to start constructing a bitmap.

 2. `BitmapBuilder` implements the trait [`tcw3::pal::iface::Canvas`]. Use the
    methods from this trait to issue 2D drawing commands.

 3. Finally, call [`tcw3::pal::BitmapBuilder::into_bitmap`] to convert the
    `BitmapBuilder` into an immutable [`tcw3::pal::Bitmap`].

[`tcw3::pal::BitmapBuilder`]: tcw3_pal::BitmapBuilder
[`::new`]: tcw3_pal::iface::BitmapBuilderNew::new
[`tcw3::pal::iface::Canvas`]: tcw3_pal::iface::Canvas
[`tcw3::pal::BitmapBuilder::into_bitmap`]: tcw3_pal::iface::BitmapBuilder::into_bitmap
[`tcw3::pal::Bitmap`]: tcw3_pal::Bitmap

### Text layout/rendering

*To be filled*

### Windows

*To be filled*

### Per-Monitor DPI

*To be filled*

### Composition Layers

*To be filled*

### Views

*To be filled*

### Styling Framework

*To be filled*

A view hierarchy and a styling element tree are independent from each other.
However, there are some points at which there is a one-by-one relationship
between them. Such points are useful for connecting widgets and thus represented
by the trait **[`Widget`]**. This trait provides two methods each returning the
root node of the corresponding type of subtree. The client of this trait can use
them to embed the widget in outer trees. For example, [`Split`] has a method
named [`set_children`] that receives two values of `&dyn Widget` and puts them
on the respective sides of a splitter. `Split` itself implements `Widget`, so
the user of `Split` can easily place a `Split` inside something that exposes a
method equivalent to `set_children`.

The following diagram illustrates this model.

```text
   view hierarchy     styling tree

        window
     ┌ ─ ─│─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ┐
     │ A_view           A_elem ├ Widget A
     └─ ─ ┼ ─ ─ ─ ─ ─ ─ ─ ─│─ ─┘
       ┌──┴──┐             │
     view  view            │
             │             │
            ─│─ ─ ─ ─ ─ ─ ─│─ ─ Widget A may expose this "socket",
             │             │    to which Widget B can be plugged in
             │             │
        ┌─ ─ ┼ ─ ─ ─ ─ ─ ─ ┼ ─ ┐
        │ B_view        B_elem ├ Widget B
        └ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─┘
```

[`Widget`]: crate::ui::theming::Widget
[`Split`]: crate::ui::views::Split
[`set_children`]: crate::ui::views::Split::set_children

Some styling elements (only [`StyledBox`] at the moment, actually) support
controlling the arrangement of their subviews through the styling framework.
Subviews are modeled by the styling framework as follows: Each styling element
is associated with a set of pairs of type `(Role, HView)`. [`Role`] is used
to identify subviews in stylesheets and to apply styling props to specific
subviews.

[`StyledBox`]: crate::ui::theming::StyledBox
[`Role`]: crate::ui::theming::Role

### Layouting Algorithm

`uicore` uses a two-phase layouting algorithm. The algoritm consists of
the following steps:

 - *Up phase*: `SizeTraits` (a triplet of min/max/preferred sizes) is
   calculated for each view in a top-down manner using the local properties
   and subviews' `SizeTraits`.

 - The window size is constrained based on the root view's `SizeTraits`. The
   root view's frame always matches the window size.

 - *Down phase*: The final frame (a bounding rectangle in the superview
   coordinate space) is calculated for each view in a bottom-up manner.

### Mouse

*To be filled*

### Keyboard

There are multiple ways for a TCW3 application to handle keyboard events, each
having different goals and purposes:

1. Define one or more *accelerator tables* (precompiled mapping from key
   combinations to action IDs) and provide the list of them through
   `WndListener::interpret_event` to have the backend translate keyboard events
   to *action IDs*, which are delivered to the currently focused view or window.
   (See the section *Actions* for more).

2. (macOS only) Create menu items with key equivalents set to the desired
   key combination and its `target` set to `nil`. The Objective C messages
   generated by activating those menu items are converted to actions using
   accelerator tables if they are provided through
   `WndListener::interpret_event`. (See the section *Actions* for more).

3. Define an accelerator table similarly, but use it in a window or view's
   keyboard event handler `*Listener::key_*` ([`pal::iface::WndListener`],
   [`uicore::WndListener`], [`uicore::ViewListener`]) to interpret the event.

   `uicore` delivers key events to a view or window. To determine the ultimate
   receiver, the `uicore` implementation of these methods examines the currently
   focused view, and moves up until the `key_down` or `key_up` methods
   ([`WndListener`], [`ViewListener`]) returns `true` for the view or window.

4. Use the text input API. The system processes input events and tells the
   application what to do by interfacing with the abstract text storage exposed
   via a text input context or by generating actions.

Upon receiving a keyboard event, the system tries each of these ways until the
event is handled or there are no more ways to try. The exact order isn't
strictly defined, but it usually looks like the following: 4 (modal only) → 2 →
1 → 3 → 4.

[`pal::iface::WndListener`]: tcw3_pal::iface::WndListener::key_down
[`uicore::WndListener`]: crate::uicore::WndListener::key_down
[`uicore::ViewListener`]: crate::uicore::ViewListener::key_down
[`WndListener`]: crate::uicore::WndListener::key_down
[`ViewListener`]: crate::uicore::ViewListener::key_down

```text
                    Keyboard Event
                          |
                          |
            ,-------------+----------------------+-----------------,
            |(3)          |(1)                   |(2)              |(4)
            |             |                      v                 v
            |             |             ,-----------------,  ,------------,
            |             |             | Key equivalents |  | Text input |
            |             |             '-----------------'  '------------'
            |             |                      | SEL           |   |
            |             |     ,----------------+               |   |
System      |             |     |                |               |   |
- - - - - - | - - - - - - | - - | - - - ,        v            , -|- -|- - -
App         |             v     v       , ,-----------------, ,  |   |
            |     ,-----------------,   , | Other UI (e.g., | ,  |   |
            |     |  WndListener::  |   , |  "open" dialog  | ,  |   |
            |     | interpret_event |   , '-----------------' ,  |   |
            |     '-----------------'    - - - - - - - - - - -   |   |
            |             | Action ID                            |   |
- - - - - - | - - - - - - | - - - - - - - - - - - - - - - - - - -|- -|- - -
Widgets     |             |           ,--------------------------'   |
            |             |           |                              |
            v             v           v                              v
,------------------, ,---------------------,       ,----------------------,
| *Listener::key_* | | *Listener::*_action |       | TextInputCtxListener |
'------------------' '---------------------'       '----------------------'
```

### Actions

Single-shot operations such as copying to clipboard are delivered to widgets as
*actions*.
Actions have global identifiers shared by all components, so they are suitable
for common UI operations and application-wide operations, but not for
widget-local operations.

Actions are generated through one of the following mechanisms:

 - The application creates one or more *accelerator tables*, which are mappings
   from key combinations to actions with a platform-specific representation.
   When the backend needs to interpret an input event, it calls
   **`WndListener::interpret_event`** ([`pal`], [`uicore`]), which calls a given
   callback function for each active accelerator table until it finds an
   applicable mapping.

[`pal`]: tcw3_pal::iface::WndListener::interpret_event
[`uicore`]: crate::uicore::WndListener::interpret_event

 - When a text input context is active, the system sends some commands as
   actions. See [`tcw3::pal::actions`] for the list of the commands that can be
   generated through this mechanism.

[`tcw3::pal::actions`]: tcw3_pal::actions

 - (macOS only) When the user selects an application menu item (the creation of
   this is out of the scope of TCW3) or inputs its key equivalent, Cocoa sends
   an Objective C message down a responder chain. If an application object
   happens to receive it, the TCW3 backend will attempt to translate it to an
   action. On macOS, accelerator tables define mappings from Objective C
   selectors to actions in addition to the aforementioned key-to-action
   mappings.

   Standard widgets from Cocoa use this responder chain as well. You can observe
   this by opening a standard file dialog and clicking the application's Edit
   menu, where you will find Cut/Copy/Paste are usable even in the dialog. Also,
   the user can customize the key equivalents of menu items in the user's system
   preference. This means you should prefer this mechanism over key-to-action
   mappings described in the previous bullet point.

<!-- TODO: The application can programmatically send actions to itself. -->

Actions are identified by 16-bit integers ([`ActionId`]). Some ranges are
reserved by TCW3 for [common UI operations].

[`ActionId`]: tcw3_pal::ActionId
[common UI operations]: tcw3_pal::actions

<!-- TODO: Application-global listener -->

The `pal` backend calls the following methods of `pal::iface::WndListener` to
perform an action or to see if an action is valid in the current state:

 - **[`validate_action`]**: Returns flags indicating such as whether the window
   can perform the action right now or not.
 - **[`perform_action`]**: Performs the action.

[`validate_action`]: tcw3_pal::iface::WndListener::validate_action
[`perform_action`]: tcw3_pal::iface::WndListener::perform_action

The `uicore` implementation of this trait forwards the calls to these methods to
a view or window. To determine the ultimate receiver, the `uicore`
implementation of these methods examines the currently focused view, and moves
up until the `validate_action` method ([`WndListener`], [`ViewListener`])
returns `ActionStatus::VALID` for the view or window.

[`WndListener`]: crate::uicore::WndListener::validate_action
[`ViewListener`]: crate::uicore::ViewListener::validate_action

### Text Input

*To be filled*

### Tab Order

The default tab order follows the pre-order of the view hierarchy. The order
for sibling views are defined by [`Layout::subviews`].

[`Layout::subviews`]: crate::uicore::Layout::subviews

The default order can be overridden by [`HViewRef::override_tab_order_sibling`]
and [`HViewRef::override_tab_order_child`]. These methods define a completely
independent subtree that determines the tab order. The client is
responsible for linking nodes correctly.

[`HViewRef::override_tab_order_sibling`]: crate::uicore::HViewRef::override_tab_order_sibling
[`HViewRef::override_tab_order_child`]: crate::uicore::HViewRef::override_tab_order_child

```rust
use tcw3::uicore::{HView, TabOrderSibling};
// root
//  ├─ v1
//  └─ v2
let root = HView::new(Default::default());
let v1 = HView::new(Default::default());
let v2 = HView::new(Default::default());
root.override_tab_order_child(Some([v1.clone(), v2.clone()]));
v1.override_tab_order_sibling(
    TabOrderSibling::Parent(root.downgrade()),
    TabOrderSibling::Sibling(v2.downgrade()),
);
v2.override_tab_order_sibling(
    TabOrderSibling::Sibling(v1.downgrade()),
    TabOrderSibling::Parent(root.downgrade()),
);
```

### Headless Backend

The **`testing`** feature enables the `testing` backend of `tcw3::pal`,
which is a headless backend that simulates the behavior of a real window
system. This is mainly used for unit testing and is supposed to be enabled
only when running tests by passing a command-line option like
`cargo test --workspace --all-features`. When the feature is not enabled,
the entry point function explained in the next paragraph will do nothing
except for outputting a warning message.

Enabling the feature alone doesn't activate the headless backend. You need
to call a specific entry point function and pass a closure. The closure
will receive `&dyn TestingWm` that can be used to send simulated input
events to the backend.
See the documentation of [`tcw3::pal::testing`] for more details.

*Note: You need to enable the `testing` feature to see the documentation.*

[`tcw3::pal::testing`]: tcw3_pal::testing

**[`tcw3::testing`]** provides an attribute macro useful for writing unit
tests using the `testing` backend.

[`tcw3::testing`]: tcw3_testing

### Color Management

Color values are specified in the sRGB color space, unless otherwise
specified.

*Full color management support is yet to be implemented. Some backends
are incapable of doing even a basic color management at the moment.*
