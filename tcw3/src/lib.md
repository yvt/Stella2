# TCW3 — Cross-platform GUI toolkit

<!-- This file is imported as the top-level doc comment of `tcw3` -->

## Architectural Overview

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
doesn't completely abstract `pal`, so widgets and applications occasionally
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

### Event Loop

### 2D Graphics

### Windows

### Per-Monitor DPI

### Composition Layers

### Views

### Mouse

### Keyboard

<!--
    TODO: Widget-local key bindings are interpreted using an accelerator table
    as well. There is no platform-neutral way to examine the contents of
    keyboard events.
-->

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

 - (macOS only) When the user selects an application menu item (the creation of
   this is out of the scope of TCW3) or inputs its key equivalent, Cocoa sends
   a Objective C message down a responder chain. If an application object
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
